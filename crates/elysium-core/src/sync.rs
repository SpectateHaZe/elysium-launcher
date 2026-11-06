//! Synced packs. The owner shares an instance straight from their PC; friends join with a
//! share code and get the owner's mods and configs before every launch.
//!
//! The owner's launcher runs a small HTTP server on [`PORT`]. It serves a manifest of the
//! instance's files and the files themselves, behind a random token that is part of the
//! share code. Mods with a public download (Modrinth, CurseForge CDN) are fetched from
//! there; everything else comes from the owner's PC.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::content::install::{ContentEntry, ContentIndex};
use crate::error::{Error, IoContext, Result};
use crate::fsutil;
use crate::http::{DownloadTask, Http, Verify};
use crate::instance::{Instance, InstanceManager, ModLoader, PackOrigin};
use crate::java::safe_join;
use crate::progress::Progress;

pub const PORT: u16 = 47630;
const SHARE_FILE: &str = "share.json";
const SYNC_FILE: &str = "sync.json";

/// Top-level folders that are the player's own, never synced.
const SKIP_DIRS: &[&str] = &[
    "saves",
    "logs",
    "crash-reports",
    "screenshots",
    "natives",
    ".fabric",
    ".cache",
    "downloads",
    "backups",
    "journeymap",
    "xaero",
    "XaeroWaypoints",
    "XaeroWorldMap",
    "schematics",
];
/// Top-level files that hold one player's own settings or history.
const SKIP_FILES: &[&str] = &[
    "options.txt",
    "optionsof.txt",
    "optionsshaders.txt",
    "usercache.json",
    "usernamecache.json",
    "realms_persistence.json",
    "command_history.txt",
    "launcher_profiles.json",
];
/// Where friends may download public files from instead of the owner's PC.
const PUBLIC_HOSTS: &[&str] = &[
    "cdn.modrinth.com",
    "edge.forgecdn.net",
    "mediafilez.forgecdn.net",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncFile {
    pub path: String,
    pub sha1: String,
    pub size: u64,
    /// A public download for the same file, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncManifest {
    pub name: String,
    pub game_version: String,
    pub loader: ModLoader,
    pub loader_version: Option<String>,
    pub files: Vec<SyncFile>,
    /// The owner's content list for the synced files (names, icons, update info).
    #[serde(default)]
    pub entries: Vec<ContentEntry>,
    /// SHA-1 of the instance's picture, if it has one.
    #[serde(default)]
    pub icon: Option<String>,
}

/// What friends paste to join: where the owner's PC is and the pack's token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShareCode {
    /// `host:port` addresses to try in order.
    pub addrs: Vec<String>,
    pub token: String,
    pub name: String,
}

impl ShareCode {
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        format!(
            "elysium-{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
        )
    }

    pub fn decode(code: &str) -> Result<Self> {
        let bad = || Error::Invalid("that isn't a share code from Elysium".into());
        let body = code.trim().strip_prefix("elysium-").ok_or_else(bad)?;
        let json = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|_| bad())?;
        let code: Self = serde_json::from_slice(&json).map_err(|_| bad())?;
        if code.addrs.is_empty() || code.token.len() < 16 {
            return Err(bad());
        }
        Ok(code)
    }
}

/// The owner's side of a shared instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShareState {
    token: String,
}

/// A friend's side: where the pack comes from and what the last sync put in place.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SyncState {
    code: Option<ShareCode>,
    /// Synced path to its SHA-1, as of the last sync.
    #[serde(default)]
    files: HashMap<String, String>,
    /// SHA-1 of the owner's picture as of the last sync.
    #[serde(default)]
    icon: Option<String>,
}

/// Whether a path (relative to the game folder, `/`-separated) belongs in a synced pack.
pub fn syncable(rel: &str) -> bool {
    let mut parts = rel.split('/');
    let Some(first) = parts.next() else {
        return false;
    };
    let nested = parts.next().is_some();
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == ".." || p.is_empty()) {
        return false;
    }
    if nested {
        !SKIP_DIRS.contains(&first)
    } else {
        !SKIP_FILES.contains(&first) && !SKIP_DIRS.contains(&first) && !first.ends_with(".part")
    }
}

/// Remembers file hashes between manifest requests so only changed files are re-read.
#[derive(Default)]
pub struct HashCache(Mutex<HashMap<PathBuf, (u64, SystemTime, String)>>);

impl HashCache {
    fn sha1(&self, path: &Path, meta: &std::fs::Metadata) -> Result<String> {
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if let Some((len, time, hash)) = self.0.lock().unwrap_or_else(|e| e.into_inner()).get(path)
            && *len == meta.len()
            && *time == modified
        {
            return Ok(hash.clone());
        }
        let hash = fsutil::sha1_file(path)?;
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.to_owned(), (meta.len(), modified, hash.clone()));
        Ok(hash)
    }
}

/// Lists everything a friend needs from this instance.
pub fn build_manifest(instance: &Instance, cache: &HashCache) -> Result<SyncManifest> {
    let game_dir = instance.game_dir();
    let index = ContentIndex::load(instance).unwrap_or_default();
    let urls: HashMap<&str, &str> = index
        .entries
        .iter()
        .filter_map(|e| Some((e.file.as_str(), e.url.as_deref()?)))
        .filter(|(_, u)| public_url(u))
        .collect();
    let mut files = Vec::new();
    let mut stack = vec![game_dir.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(&game_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Checked with a placeholder child so skipped folders aren't walked.
                if syncable(&format!("{rel}/x")) {
                    stack.push(path);
                }
                continue;
            }
            if !syncable(&rel) {
                continue;
            }
            let sha1 = cache.sha1(&path, &meta)?;
            // An entry matches the file whether or not the owner disabled it.
            let key = rel.strip_suffix(".disabled").unwrap_or(&rel);
            files.push(SyncFile {
                url: urls.get(key).map(|u| (*u).to_owned()),
                path: rel,
                sha1,
                size: meta.len(),
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let entries = index
        .entries
        .into_iter()
        .filter(|e| {
            files
                .iter()
                .any(|f| f.path.strip_suffix(".disabled").unwrap_or(&f.path) == e.file)
        })
        .collect();
    Ok(SyncManifest {
        name: instance.config.name.clone(),
        game_version: instance.config.game_version.clone(),
        loader: instance.config.loader,
        loader_version: instance.config.loader_version.clone(),
        icon: match instance.icon_path() {
            Some(p) => Some(cache.sha1(&p, &std::fs::metadata(&p).at(&p)?)?),
            None => None,
        },
        files,
        entries,
    })
}

fn public_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https" && u.host_str().is_some_and(|h| PUBLIC_HOSTS.contains(&h))
    })
}

// ---- Owner side ----

/// The instance's share token, creating one the first time it's shared.
pub fn start_sharing(instance: &Instance) -> Result<String> {
    let path = instance.dir.join(SHARE_FILE);
    if let Some(s) = fsutil::read_json::<ShareState>(&path)? {
        return Ok(s.token);
    }
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    fsutil::write_json_atomic(
        &path,
        &ShareState {
            token: token.clone(),
        },
    )?;
    Ok(token)
}

/// Stops sharing. Old share codes stop working; sharing again makes a new one.
pub fn stop_sharing(instance: &Instance) -> Result<()> {
    let path = instance.dir.join(SHARE_FILE);
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).at(&path),
        _ => Ok(()),
    }
}

pub fn is_shared(instance: &Instance) -> bool {
    instance.dir.join(SHARE_FILE).is_file()
}

fn token_of(instance: &Instance) -> Option<String> {
    fsutil::read_json::<ShareState>(&instance.dir.join(SHARE_FILE))
        .ok()
        .flatten()
        .map(|s| s.token)
}

/// How friends can reach this PC.
#[derive(Debug, Clone, Serialize)]
pub struct Reach {
    /// Addresses for the share code, internet first.
    pub addrs: Vec<String>,
    /// Whether the router opened the port by itself (UPnP). If not, friends outside the
    /// owner's network can only connect once the owner forwards [`PORT`].
    pub opened: bool,
}

/// Works out the addresses friends can use, asking the router to open [`PORT`] if it can.
pub async fn reach(http: &Http) -> Reach {
    let lan = local_ip();
    let mut addrs = Vec::new();
    let mut opened = false;
    if let Some(lan) = lan
        && let Ok(Ok(gateway)) = tokio::time::timeout(
            Duration::from_secs(6),
            igd_next::aio::tokio::search_gateway({
                let mut o = igd_next::SearchOptions::default();
                o.timeout = Some(Duration::from_secs(5));
                o
            }),
        )
        .await
    {
        let local = SocketAddr::new(lan, PORT);
        let proto = igd_next::PortMappingProtocol::TCP;
        // Some routers refuse permanent mappings, so fall back to a long lease.
        let mapped = gateway
            .add_port(proto, PORT, local, 0, "Elysium pack sync")
            .await
            .is_ok()
            || gateway
                .add_port(proto, PORT, local, 7 * 24 * 3600, "Elysium pack sync")
                .await
                .is_ok();
        if mapped && let Ok(ip) = gateway.get_external_ip().await {
            addrs.push(SocketAddr::new(ip, PORT).to_string());
            opened = true;
        }
    }
    if !opened
        && let Ok(ip) = http.get_bytes("https://api.ipify.org").await
        && let Ok(ip) = String::from_utf8_lossy(&ip).trim().parse::<IpAddr>()
    {
        addrs.push(SocketAddr::new(ip, PORT).to_string());
    }
    if let Some(lan) = lan {
        addrs.push(SocketAddr::new(lan, PORT).to_string());
    }
    Reach { addrs, opened }
}

/// The address this PC uses on its local network.
fn local_ip() -> Option<IpAddr> {
    // Connecting a UDP socket sends nothing; it only picks the outgoing interface.
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

/// Serves every shared instance on `port` (0 picks a free one) until the process exits.
/// Returns the port it listens on.
pub async fn serve(instances: InstanceManager, port: u16) -> Result<u16> {
    let listener = TcpListener::bind(("0.0.0.0", port))
        .await
        .map_err(|e| Error::Invalid(format!("couldn't open port {port} for sharing: {e}")))?;
    let port = listener.local_addr().at("sync server")?.port();
    let cache = Arc::new(HashCache::default());
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let (instances, cache) = (instances.clone(), Arc::clone(&cache));
            tokio::spawn(async move {
                if let Err(e) = handle(stream, &instances, &cache).await {
                    tracing::debug!("sync request failed: {e}");
                }
            });
        }
    });
    Ok(port)
}

async fn handle(
    mut stream: TcpStream,
    instances: &InstanceManager,
    cache: &HashCache,
) -> Result<()> {
    let mut buf = vec![0u8; 8192];
    let mut len = 0;
    let head = loop {
        let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf[len..]))
            .await
            .map_err(|_| Error::Invalid("request timed out".into()))?
            .at("sync request")?;
        if n == 0 {
            return Ok(());
        }
        len += n;
        if let Some(end) = buf[..len].windows(4).position(|w| w == b"\r\n\r\n") {
            break String::from_utf8_lossy(&buf[..end]).into_owned();
        }
        if len == buf.len() {
            return respond(&mut stream, 431, "text/plain", b"too large").await;
        }
    };
    let target = head
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("GET "))
        .and_then(|l| l.split(' ').next())
        .unwrap_or("");
    let Ok(url) = url::Url::parse(&format!("http://pack{target}")) else {
        return respond(&mut stream, 400, "text/plain", b"bad request").await;
    };
    let segs: Vec<&str> = url
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    let (["v1", token, route], true) = (segs.as_slice(), target.starts_with('/')) else {
        return respond(&mut stream, 404, "text/plain", b"not found").await;
    };
    let instance = instances
        .list()
        .unwrap_or_default()
        .into_iter()
        .find(|i| token_of(i).is_some_and(|t| constant_eq(&t, token)));
    let Some(instance) = instance else {
        return respond(
            &mut stream,
            404,
            "text/plain",
            b"this pack isn't shared anymore",
        )
        .await;
    };
    match *route {
        "manifest" => {
            let inst = instance.clone();
            let manifest = tokio::task::block_in_place(|| build_manifest(&inst, cache))?;
            let json = serde_json::to_vec(&manifest).map_err(|source| Error::Json {
                context: "sync manifest".into(),
                source,
            })?;
            respond(&mut stream, 200, "application/json", &json).await
        }
        "icon" => match instance.icon_path().map(std::fs::read) {
            Some(Ok(bytes)) => respond(&mut stream, 200, "application/octet-stream", &bytes).await,
            _ => respond(&mut stream, 404, "text/plain", b"no picture").await,
        },
        "file" => {
            let rel = url
                .query_pairs()
                .find(|(k, _)| k == "p")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default();
            let path = (syncable(&rel))
                .then(|| safe_join(&instance.game_dir(), &rel).ok())
                .flatten()
                .filter(|p| p.is_file());
            let Some(path) = path else {
                return respond(&mut stream, 404, "text/plain", b"no such file").await;
            };
            let mut file = tokio::fs::File::open(&path).await.at(&path)?;
            let size = file.metadata().await.at(&path)?.len();
            stream
                .write_all(
                    format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                )
                .await
                .at("sync response")?;
            tokio::io::copy(&mut file, &mut stream).await.at(&path)?;
            stream.shutdown().await.ok();
            Ok(())
        }
        _ => respond(&mut stream, 404, "text/plain", b"not found").await,
    }
}

fn constant_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0, |acc, (x, y)| acc | (x ^ y))
            == 0
}

async fn respond(stream: &mut TcpStream, status: u16, kind: &str, body: &[u8]) -> Result<()> {
    let reason = if status == 200 { "OK" } else { "Error" };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .await
        .at("sync response")?;
    stream.write_all(body).await.at("sync response")?;
    stream.shutdown().await.ok();
    Ok(())
}

// ---- Friend side ----

/// Whether this instance follows someone else's pack.
pub fn is_synced(instance: &Instance) -> bool {
    instance.dir.join(SYNC_FILE).is_file()
}

/// Fetches the owner's manifest from the first address that answers.
async fn fetch(http: &Http, code: &ShareCode) -> Result<(String, SyncManifest)> {
    let mut last = None;
    for addr in &code.addrs {
        let url = format!("http://{addr}/v1/{}/manifest", code.token);
        let r = tokio::time::timeout(Duration::from_secs(8), http.client().get(&url).send()).await;
        match r {
            Ok(Ok(resp)) if resp.status().is_success() => {
                let manifest = resp.json::<SyncManifest>().await?;
                return Ok((addr.clone(), manifest));
            }
            Ok(Ok(resp)) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                return Err(Error::Invalid(format!(
                    "{} isn't shared anymore. Ask for a new share code.",
                    code.name
                )));
            }
            Ok(Ok(resp)) => last = Some(format!("the owner's PC answered {}", resp.status())),
            Ok(Err(e)) => last = Some(e.to_string()),
            Err(_) => last = Some("no answer".into()),
        }
    }
    Err(Error::Invalid(format!(
        "couldn't reach the PC sharing {}. It needs Elysium open ({})",
        code.name,
        last.unwrap_or_default()
    )))
}

/// Creates an instance that follows a shared pack, then syncs it.
pub async fn join(
    http: &Http,
    instances: &InstanceManager,
    code: &str,
    concurrency: usize,
    progress: Arc<Progress>,
) -> Result<String> {
    let code = ShareCode::decode(code)?;
    progress.set_stage(format!("Connecting to {}", code.name), 0);
    let (_, manifest) = fetch(http, &code).await?;
    let instance = instances.create_with(
        &manifest.name,
        &manifest.game_version,
        manifest.loader,
        manifest.loader_version.clone(),
    )?;
    let instance = instances.update(&instance.id, |c| {
        c.pack = Some(PackOrigin {
            source: "sync".into(),
            project_id: None,
            version_id: None,
            name: manifest.name.clone(),
        });
    })?;
    fsutil::write_json_atomic(
        &instance.dir.join(SYNC_FILE),
        &SyncState {
            code: Some(code),
            ..Default::default()
        },
    )?;
    if let Err(e) = sync(http, instances, &instance.id, concurrency, progress).await {
        let _ = instances.delete(&instance.id);
        return Err(e);
    }
    Ok(instance.id)
}

/// What a sync changed.
#[derive(Debug, Default, Clone, Serialize)]
pub struct SyncReport {
    pub updated: usize,
    pub removed: usize,
}

/// Brings a synced instance up to date with the owner's pack.
pub async fn sync(
    http: &Http,
    instances: &InstanceManager,
    id: &str,
    concurrency: usize,
    progress: Arc<Progress>,
) -> Result<SyncReport> {
    let instance = instances.get(id)?;
    let state_path = instance.dir.join(SYNC_FILE);
    let mut state: SyncState = fsutil::read_json(&state_path)?.unwrap_or_default();
    let Some(mut code) = state.code.clone() else {
        return Ok(SyncReport::default());
    };
    progress.set_stage(format!("Checking {} for changes", code.name), 0);
    let (addr, manifest) = fetch(http, &code).await?;
    // Try the address that worked first next time.
    code.addrs.retain(|a| *a != addr);
    code.addrs.insert(0, addr.clone());

    let c = &instance.config;
    if c.game_version != manifest.game_version
        || c.loader != manifest.loader
        || c.loader_version != manifest.loader_version
    {
        instances.update(id, |c| {
            c.game_version = manifest.game_version.clone();
            c.loader = manifest.loader;
            c.loader_version = manifest.loader_version.clone();
        })?;
    }

    let game_dir = instance.game_dir();
    let mut tasks = Vec::new();
    for f in &manifest.files {
        if !syncable(&f.path) {
            continue;
        }
        let dest = safe_join(&game_dir, &f.path)?;
        let current = state.files.get(&f.path) == Some(&f.sha1)
            && std::fs::metadata(&dest).is_ok_and(|m| m.len() == f.size);
        if current {
            continue;
        }
        let url = match &f.url {
            Some(u) if public_url(u) => u.clone(),
            _ => {
                let p: String = url::form_urlencoded::byte_serialize(f.path.as_bytes()).collect();
                format!("http://{addr}/v1/{}/file?p={p}", code.token)
            }
        };
        tasks.push(
            DownloadTask::new(url, &dest)
                .sha1(Some(f.sha1.clone()))
                .size(Some(f.size)),
        );
    }
    let updated = tasks.len();
    progress.set_stage(format!("Syncing {}", code.name), 0);
    http.download_all(tasks, concurrency, Verify::Hash, Arc::clone(&progress))
        .await?;

    // Files the owner removed. Files the player added themselves are left alone.
    let keep: std::collections::HashSet<&str> =
        manifest.files.iter().map(|f| f.path.as_str()).collect();
    let mut removed = 0;
    for path in state.files.keys().filter(|p| !keep.contains(p.as_str())) {
        if let Ok(full) = safe_join(&game_dir, path)
            && std::fs::remove_file(&full).is_ok()
        {
            removed += 1;
        }
    }

    // The content list: the owner's entries for synced files, plus the player's own.
    let mut index = ContentIndex::load(&instance).unwrap_or_default();
    let synced_before: std::collections::HashSet<&str> = state
        .files
        .keys()
        .map(|p| p.strip_suffix(".disabled").unwrap_or(p))
        .collect();
    let synced_now: std::collections::HashSet<&str> = manifest
        .files
        .iter()
        .map(|f| f.path.strip_suffix(".disabled").unwrap_or(&f.path))
        .collect();
    index.entries.retain(|e| {
        !synced_before.contains(e.file.as_str()) && !synced_now.contains(e.file.as_str())
    });
    index.entries.extend(manifest.entries.iter().cloned());
    index.save(&instance)?;

    // The owner's picture. `set_icon` checks it's a real image of sensible size.
    if manifest.icon != state.icon {
        match &manifest.icon {
            Some(_) => {
                let bytes = http
                    .get_bytes(&format!("http://{addr}/v1/{}/icon", code.token))
                    .await?;
                instances.set_icon(id, &bytes)?;
            }
            None => {
                instances.clear_icon(id)?;
            }
        }
        state.icon = manifest.icon.clone();
    }

    state.code = Some(code);
    state.files = manifest
        .files
        .iter()
        .map(|f| (f.path.clone(), f.sha1.clone()))
        .collect();
    fsutil::write_json_atomic(&state_path, &state)?;
    Ok(SyncReport { updated, removed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_codes_round_trip() {
        let code = ShareCode {
            addrs: vec!["1.2.3.4:47630".into(), "192.168.1.5:47630".into()],
            token: "0123456789abcdef0123456789abcdef".into(),
            name: "Friends pack".into(),
        };
        assert_eq!(ShareCode::decode(&code.encode()).unwrap(), code);
        assert!(ShareCode::decode("hello").is_err());
        assert!(ShareCode::decode("elysium-!!").is_err());
    }

    #[test]
    fn only_pack_files_sync() {
        assert!(syncable("mods/sodium.jar"));
        assert!(syncable("config/sodium-options.json"));
        assert!(syncable("servers.dat"));
        assert!(!syncable("options.txt"));
        assert!(!syncable("saves/world/level.dat"));
        assert!(!syncable("mods/../options.txt"));
        assert!(!syncable("/etc/passwd"));
        assert!(!syncable(""));
        assert!(!syncable("config//x"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn friend_follows_owner() {
        let tmp = tempfile::tempdir().unwrap();
        let owner = InstanceManager::new(tmp.path().join("owner"));
        let friend = InstanceManager::new(tmp.path().join("friend"));
        let pack = owner.create("Pack", "1.21.1").unwrap();
        let dir = pack.game_dir();
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        std::fs::create_dir_all(dir.join("config")).unwrap();
        std::fs::write(dir.join("mods/a.jar"), b"mod a").unwrap();
        std::fs::write(dir.join("config/a.toml"), b"x = 1").unwrap();
        std::fs::write(dir.join("options.txt"), b"mine").unwrap();
        let png = b"\x89PNG\r\n\x1a\nfake picture";
        owner.set_icon(&pack.id, png).unwrap();
        let token = start_sharing(&pack).unwrap();
        let port = serve(owner.clone(), 0).await.unwrap();
        let code = ShareCode {
            addrs: vec![format!("127.0.0.1:{port}")],
            token,
            name: "Pack".into(),
        }
        .encode();

        let http = Http::new().unwrap();
        let id = join(&http, &friend, &code, 4, Arc::new(Progress::new()))
            .await
            .unwrap();
        let mine = friend.get(&id).unwrap().game_dir();
        assert_eq!(std::fs::read(mine.join("mods/a.jar")).unwrap(), b"mod a");
        assert!(!mine.join("options.txt").exists());
        let icon = friend.get(&id).unwrap().icon_path().unwrap();
        assert_eq!(std::fs::read(icon).unwrap(), png);

        // The owner swaps a mod and edits a config; a friend's own mod stays.
        std::fs::remove_file(dir.join("mods/a.jar")).unwrap();
        std::fs::write(dir.join("mods/b.jar"), b"mod b").unwrap();
        std::fs::write(dir.join("config/a.toml"), b"x = 2").unwrap();
        std::fs::write(mine.join("mods/mine.jar"), b"own").unwrap();
        let r = sync(&http, &friend, &id, 4, Arc::new(Progress::new()))
            .await
            .unwrap();
        assert_eq!((r.updated, r.removed), (2, 1));
        assert!(!mine.join("mods/a.jar").exists());
        assert_eq!(std::fs::read(mine.join("mods/b.jar")).unwrap(), b"mod b");
        assert_eq!(std::fs::read(mine.join("config/a.toml")).unwrap(), b"x = 2");
        assert!(mine.join("mods/mine.jar").exists());

        owner.clear_icon(&pack.id).unwrap();
        sync(&http, &friend, &id, 4, Arc::new(Progress::new()))
            .await
            .unwrap();
        assert!(friend.get(&id).unwrap().icon_path().is_none());

        stop_sharing(&pack).unwrap();
        assert!(
            sync(&http, &friend, &id, 4, Arc::new(Progress::new()))
                .await
                .is_err()
        );
    }
}
