//! Horizon: the Rust reimplementation of Minecraft: Java Edition
//! (<https://github.com/horizon-rust/horizon>), run through its documented Launch API.
//!
//! - Builds are the GitHub release tagged `nightly`. Its exact version
//!   (`nightly-<date>-<commit>`) comes from the repository's tags, and every download is
//!   checked against `SHA256SUMS`, whose Ed25519 signature (`SHA256SUMS.v2.sig`) must
//!   name that exact version and verify with Horizon's release key.
//! - Horizon contains no Mojang code or assets. It prepares them from the player's own
//!   game files, so it is only offered to signed-in owners of Java Edition, as Horizon
//!   requires of launchers.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest as _;
use tokio::io::AsyncBufReadExt;

use crate::account::LaunchSession;
use crate::error::{Error, IoContext, Result};
use crate::fsutil;
use crate::http::{DownloadTask, Http, Verify};
use crate::install::{self, InstallOptions};
use crate::instance::Instance;
use crate::launch::{LaunchCommand, hide_console};
use crate::paths::Paths;
use crate::progress::Progress;
use crate::settings::EffectiveSettings;

pub const REPO: &str = "https://github.com/horizon-rust/horizon";

/// Horizon's release public keys (crates/horizon-release-sign/release-keys.txt).
pub const RELEASE_KEYS: &[&str] = &["MU4oiIQcCxkZvbwSjjXs7SySCuM6ywRQ3nGaJYZG0gw="];

const SUPPORTED_API: &str = "0.1.";

/// Archive name for this platform, or `None` where Horizon has no build.
pub fn platform_archive() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("horizon-windows-x64.zip"),
        ("windows", "aarch64") => Some("horizon-windows-arm64.zip"),
        ("linux", "x86_64") => Some("horizon-linux-x86_64.tar.gz"),
        ("macos", "aarch64") => Some("horizon-macos-arm64.zip"),
        ("macos", "x86_64") => Some("horizon-macos-x64.zip"),
        _ => None,
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

/// Finds the version of the current nightly from the repository's tags:
/// `refs/tags/nightly` points at a commit, and `refs/tags/nightly-source/<date>-<sha7>`
/// names its date.
pub fn parse_nightly_version(refs: &str) -> Option<String> {
    let mut nightly = None;
    let mut sources = Vec::new();
    for line in refs.lines() {
        // pkt-line framing: 4 hex length digits, then "<sha> <ref>[\0capabilities]".
        let line = line.split('\0').next().unwrap_or(line);
        let Some(pos) = line.find(" refs/") else {
            continue;
        };
        let sha = line[..pos]
            .chars()
            .rev()
            .take(40)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        let name = line[pos + 1..].trim();
        let (name, _peeled) = match name.strip_suffix("^{}") {
            Some(n) => (n, true),
            None => (name, false),
        };
        if name == "refs/tags/nightly" {
            nightly = Some(sha);
        } else if let Some(rest) = name.strip_prefix("refs/tags/nightly-source/") {
            sources.push((sha, rest.to_owned()));
        }
    }
    let nightly = nightly?;
    let (_, rest) = sources
        .into_iter()
        .find(|(sha, rest)| *sha == nightly || rest.ends_with(&nightly[..7]))?;
    Some(format!("nightly-{rest}"))
}

/// Verifies `SHA256SUMS` against a signature over `horizon release <version>\n` + sums.
pub fn verify_sums(sums: &[u8], signature_b64: &str, version: &str) -> Result<()> {
    let sig = base64::engine::general_purpose::STANDARD
        .decode(signature_b64.trim())
        .map_err(|_| Error::Invalid("Horizon's checksum signature isn't valid base64".into()))?;
    let sig = ed25519_dalek::Signature::from_slice(&sig)
        .map_err(|_| Error::Invalid("malformed Horizon signature".into()))?;
    let mut message = format!("horizon release {version}\n").into_bytes();
    message.extend_from_slice(sums);
    for key in RELEASE_KEYS {
        let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(key) else {
            continue;
        };
        let Ok(raw): std::result::Result<[u8; 32], _> = raw.try_into() else {
            continue;
        };
        let Ok(vk) = ed25519_dalek::VerifyingKey::from_bytes(&raw) else {
            continue;
        };
        if vk.verify_strict(&message, &sig).is_ok() {
            return Ok(());
        }
    }
    Err(Error::Invalid(format!(
        "Horizon {version}'s checksums aren't signed by Horizon's release key; not installing it"
    )))
}

/// SHA-256 listed for `file` in a `SHA256SUMS` text.
pub fn sum_for<'a>(sums: &'a str, file: &str) -> Option<&'a str> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file).then_some(hash)
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HorizonState {
    pub current: Option<String>,
    pub previous: Option<String>,
}

fn root(paths: &Paths) -> PathBuf {
    paths.root().join("horizon")
}

fn build_dir(paths: &Paths, version: &str) -> PathBuf {
    root(paths).join("builds").join(version)
}

pub fn state(paths: &Paths) -> Result<HorizonState> {
    Ok(fsutil::read_json(&root(paths).join("state.json"))?.unwrap_or_default())
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub installed: Option<String>,
    pub latest: Option<String>,
    pub update_available: bool,
    pub supported: bool,
}

pub async fn latest_version(http: &Http) -> Result<String> {
    let bytes = http
        .get_bytes(&format!("{REPO}.git/info/refs?service=git-upload-pack"))
        .await?;
    parse_nightly_version(&String::from_utf8_lossy(&bytes))
        .ok_or_else(|| Error::Invalid("couldn't find Horizon's current nightly".into()))
}

pub async fn check(paths: &Paths, http: &Http) -> Result<UpdateStatus> {
    let installed = state(paths)?.current;
    if platform_archive().is_none() {
        return Ok(UpdateStatus {
            installed,
            latest: None,
            update_available: false,
            supported: false,
        });
    }
    let latest = latest_version(http).await?;
    Ok(UpdateStatus {
        update_available: installed.as_deref() != Some(latest.as_str()),
        installed,
        latest: Some(latest),
        supported: true,
    })
}

/// Downloads, verifies and unpacks the newest nightly. Returns its version.
pub async fn install_latest(paths: &Paths, http: &Http, progress: Arc<Progress>) -> Result<String> {
    let archive_name = platform_archive()
        .ok_or_else(|| Error::Invalid("Horizon has no build for this platform".into()))?;
    progress.set_stage("Checking for Horizon updates", 0);
    let version = latest_version(http).await?;
    if state(paths)?.current.as_deref() == Some(version.as_str())
        && build_dir(paths, &version)
            .join(exe("horizon-client"))
            .exists()
    {
        return Ok(version);
    }
    let base = format!("{REPO}/releases/download/nightly");
    let sums = http.get_bytes(&format!("{base}/SHA256SUMS")).await?;
    let sig = http.get_bytes(&format!("{base}/SHA256SUMS.v2.sig")).await?;
    verify_sums(&sums, &String::from_utf8_lossy(&sig), &version)?;
    let sums = String::from_utf8_lossy(&sums).into_owned();
    let expected = sum_for(&sums, archive_name)
        .ok_or_else(|| Error::Invalid(format!("Horizon {version} has no {archive_name}")))?
        .to_lowercase();

    progress.set_stage(format!("Downloading Horizon {version}"), 0);
    let download = root(paths).join("downloads").join(archive_name);
    http.download_all(
        vec![DownloadTask::new(
            format!("{base}/{archive_name}"),
            &download,
        )],
        1,
        Verify::Size,
        Arc::clone(&progress),
    )
    .await?;
    let actual = {
        let data = std::fs::read(&download).at(&download)?;
        fsutil::hex(&sha2::Sha256::digest(&data))
    };
    if actual != expected {
        let _ = std::fs::remove_file(&download);
        return Err(Error::Checksum {
            url: archive_name.into(),
            expected,
            actual,
        });
    }

    progress.set_stage("Unpacking Horizon", 0);
    let dest = build_dir(paths, &version);
    let staging = dest.with_extension("partial");
    let (download2, staging2) = (download.clone(), staging.clone());
    tokio::task::spawn_blocking(move || unpack(&download2, &staging2))
        .await
        .map_err(|e| Error::Invalid(format!("unpacking failed: {e}")))??;
    // Some archives wrap everything in one folder; use the folder holding the client.
    let content = find_build_root(&staging)
        .ok_or_else(|| Error::Invalid("the Horizon archive has no horizon-client".into()))?;
    if dest.exists() {
        std::fs::remove_dir_all(&dest).at(&dest)?;
    }
    std::fs::rename(&content, &dest).at(&dest)?;
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_file(&download);

    // Keep the previous build for going back; remove anything older.
    let mut st = state(paths)?;
    if st.current.as_deref() != Some(version.as_str()) {
        if let Some(old) = st.previous.take()
            && old != version
        {
            let _ = std::fs::remove_dir_all(build_dir(paths, &old));
        }
        st.previous = st.current.take();
        st.current = Some(version.clone());
    }
    fsutil::write_json_atomic(&root(paths).join("state.json"), &st)?;
    Ok(version)
}

fn unpack(archive: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        std::fs::remove_dir_all(dest).at(dest)?;
    }
    std::fs::create_dir_all(dest).at(dest)?;
    let name = archive.to_string_lossy();
    if name.ends_with(".tar.gz") {
        let file = std::fs::File::open(archive).at(archive)?;
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        tar.set_preserve_permissions(true);
        for entry in tar.entries().at(archive)? {
            let mut entry = entry.at(archive)?;
            // `unpack_in` refuses paths that escape `dest`.
            entry.unpack_in(dest).at(dest)?;
        }
    } else {
        let file = std::fs::File::open(archive).at(archive)?;
        let mut zip = zip::ZipArchive::new(file).map_err(|source| Error::Zip {
            path: archive.to_owned(),
            source,
        })?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|source| Error::Zip {
                path: archive.to_owned(),
                source,
            })?;
            let Some(rel) = entry.enclosed_name() else {
                continue;
            };
            let out = dest.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out).at(&out)?;
                continue;
            }
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p).at(p)?;
            }
            let mut f = std::fs::File::create(&out).at(&out)?;
            std::io::copy(&mut entry, &mut f).at(&out)?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode));
            }
        }
    }
    Ok(())
}

fn find_build_root(dir: &Path) -> Option<PathBuf> {
    let client = exe("horizon-client");
    if dir.join(&client).exists() {
        return Some(dir.to_owned());
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .find(|p| p.join(&client).exists())
}

/// A build's answers to `--launch-api`.
#[derive(Debug, Clone, Serialize)]
pub struct BuildInfo {
    pub version: String,
    pub dir: PathBuf,
    pub minecraft: String,
    pub java_major: Option<u32>,
    pub client_options: Vec<String>,
    pub prepare_options: Vec<String>,
}

async fn launch_api(bin: &Path) -> Result<Value> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.arg("--launch-api")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    hide_console(&mut cmd);
    let out = tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| Error::Invalid(format!("{} didn't answer", bin.display())))?
        .at(bin)?;
    if !out.status.success() {
        return Err(Error::Invalid(format!(
            "{} --launch-api failed ({})",
            bin.display(),
            out.status
        )));
    }
    serde_json::from_slice(&out.stdout).map_err(|source| Error::Json {
        context: "--launch-api".into(),
        source,
    })
}

fn option_names(v: &Value) -> Vec<String> {
    v.get("options")
        .and_then(Value::as_array)
        .map(|o| {
            o.iter()
                .filter_map(|x| x.get("name")?.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

pub async fn build_info(paths: &Paths) -> Result<BuildInfo> {
    let version = state(paths)?
        .current
        .ok_or_else(|| Error::Invalid("Horizon isn't installed yet".into()))?;
    let dir = build_dir(paths, &version);
    let client = launch_api(&dir.join(exe("horizon-client"))).await?;
    let prepare = launch_api(&dir.join(exe("horizon-prepare"))).await?;
    let api = client
        .get("launch_api")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !api.starts_with(SUPPORTED_API) {
        return Err(Error::Invalid(format!(
            "this Horizon build uses Launch API {api}; update Elysium to run it"
        )));
    }
    Ok(BuildInfo {
        minecraft: prepare
            .get("minecraft")
            .or_else(|| client.get("minecraft"))
            .and_then(Value::as_str)
            .unwrap_or("26.3")
            .to_owned(),
        java_major: prepare
            .get("java_major")
            .and_then(Value::as_u64)
            .map(|v| v as u32),
        client_options: option_names(&client),
        prepare_options: option_names(&prepare),
        version,
        dir,
    })
}

fn prepared_dir(paths: &Paths, build: &BuildInfo) -> PathBuf {
    root(paths).join("prepared").join(&build.version)
}

/// Makes sure the build's resources are prepared from the owner's game files.
pub async fn ensure_prepared(
    paths: &Paths,
    http: &Http,
    build: &BuildInfo,
    owner: bool,
    options: &InstallOptions,
    progress: Arc<Progress>,
) -> Result<PathBuf> {
    if !owner {
        return Err(Error::Invalid(
            "Horizon needs a Microsoft account that owns Minecraft: Java Edition".into(),
        ));
    }
    let output = prepared_dir(paths, build);
    let prepare = build.dir.join(exe("horizon-prepare"));
    let mut check = tokio::process::Command::new(&prepare);
    check
        .args(["--check", "--output"])
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console(&mut check);
    if check.status().await.at(&prepare)?.success() {
        return Ok(output);
    }

    // The owner's Minecraft files for the version this build needs, plus a Java for it.
    let game = install::install_version(
        paths,
        http,
        &build.minecraft,
        options,
        Arc::clone(&progress),
    )
    .await?;
    let server = game
        .version
        .downloads
        .as_ref()
        .and_then(|d| d.server.clone())
        .ok_or_else(|| {
            Error::Invalid(format!(
                "Minecraft {} has no server download",
                build.minecraft
            ))
        })?;
    let server_jar = root(paths)
        .join("server")
        .join(format!("{}.jar", build.minecraft));
    progress.set_stage(
        "Downloading the server files Horizon prepares from",
        server.size.unwrap_or(0),
    );
    http.download_all(
        vec![
            DownloadTask::new(&server.url, &server_jar)
                .sha1(server.sha1.clone())
                .size(server.size),
        ],
        1,
        Verify::Hash,
        Arc::clone(&progress),
    )
    .await?;
    let java_home = game
        .java
        .parent()
        .and_then(Path::parent)
        .map(Path::to_owned);

    let mut args: Vec<std::ffi::OsString> = vec![
        "--prepare".into(),
        "--server-jar".into(),
        server_jar.into_os_string(),
    ];
    let mut add = |flag: &str, value: std::ffi::OsString| {
        if build.prepare_options.is_empty() || build.prepare_options.iter().any(|o| o == flag) {
            args.push(flag.into());
            args.push(value);
        }
    };
    add(
        "--version-json",
        paths.version_json(&build.minecraft).into_os_string(),
    );
    add(
        "--client-jar",
        paths.version_jar(&build.minecraft).into_os_string(),
    );
    add("--libraries-dir", paths.libraries_dir().into_os_string());
    add("--assets-dir", paths.assets_dir().into_os_string());
    if let Some(home) = java_home {
        add("--java-home", home.into_os_string());
    }
    args.extend([
        "--output".into(),
        output.clone().into_os_string(),
        "--complete".into(),
        "--progress".into(),
        "json".into(),
    ]);

    progress.set_stage(
        "Preparing Horizon (the first time takes a few minutes)",
        1000,
    );
    let mut cmd = tokio::process::Command::new(&prepare);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    hide_console(&mut cmd);
    let mut child = cmd.spawn().at(&prepare)?;
    let mut last_error = None;
    if let Some(out) = child.stdout.take() {
        let mut lines = tokio::io::BufReader::new(out).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if progress.is_cancelled() {
                let _ = child.start_kill();
                return Err(Error::Cancelled);
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(f) = v.get("fraction").and_then(Value::as_f64) {
                let (done, _) = progress.counts();
                progress.advance(((f.clamp(0.0, 1.0) * 1000.0) as u64).saturating_sub(done));
            } else if let Some(m) = v.get("message").and_then(Value::as_str) {
                tracing::info!("horizon-prepare: {m}");
            } else if let Some(e) = v.get("error").and_then(Value::as_str) {
                last_error = Some(e.to_owned());
            }
        }
    }
    let status = child.wait().await.at(&prepare)?;
    if !status.success() {
        return Err(Error::Invalid(format!(
            "Horizon couldn't prepare the game files: {}",
            last_error.unwrap_or_else(|| status.to_string())
        )));
    }
    Ok(output)
}

/// The command that starts Horizon for `instance`; the session goes over stdin.
pub fn build_command(
    build: &BuildInfo,
    resources: &Path,
    instance: &Instance,
    session: &LaunchSession,
    settings: &EffectiveSettings,
) -> LaunchCommand {
    let has = |o: &str| build.client_options.iter().any(|x| x == o);
    let mut args: Vec<String> = vec![
        "--resources".into(),
        resources.to_string_lossy().into_owned(),
        "--game-dir".into(),
        instance.game_dir().to_string_lossy().into_owned(),
        "--home".into(),
        instance
            .dir
            .join("horizon-home")
            .to_string_lossy()
            .into_owned(),
    ];
    if let Some(r) = settings.resolution
        && has("--width")
        && has("--height")
    {
        args.extend([
            "--width".into(),
            r.width.to_string(),
            "--height".into(),
            r.height.to_string(),
        ]);
    }
    args.push("--session-stdin".into());
    let session_json = serde_json::json!({
        "name": session.username,
        "uuid": session.uuid.simple().to_string(),
        "access_token": session.access_token,
        "user_type": session.user_type,
        "xuid": session.xuid,
    });
    LaunchCommand {
        program: build.dir.join(exe("horizon-client")),
        args,
        cwd: instance.game_dir(),
        env: Vec::new(),
        stdin: Some(session_json.to_string()),
    }
}

/// Explains Horizon's documented exit codes.
pub fn exit_message(code: Option<i32>) -> Option<&'static str> {
    match code {
        Some(2) => Some("Horizon refused the command line; please report this"),
        Some(1) | Some(101) => {
            Some("Horizon stopped with an error; the log names the crash report")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMS: &[u8] = include_bytes!("../tests/fixtures/horizon/SHA256SUMS");
    const SIG_V1: &str = include_str!("../tests/fixtures/horizon/SHA256SUMS.sig");
    const SIG_V2: &str = include_str!("../tests/fixtures/horizon/SHA256SUMS.v2.sig");

    #[test]
    fn verifies_real_release_signatures() {
        verify_sums(SUMS, SIG_V2, "nightly-2026-11-05-66f3981").unwrap();
        verify_sums(SUMS, SIG_V1, "nightly").unwrap();
        // A signature for one version can't be replayed for another.
        assert!(verify_sums(SUMS, SIG_V2, "nightly-2026-11-04-6475f62").is_err());
        // Any change to the list breaks it.
        let mut tampered = SUMS.to_vec();
        tampered[0] ^= 1;
        assert!(verify_sums(&tampered, SIG_V2, "nightly-2026-11-05-66f3981").is_err());
        assert!(verify_sums(SUMS, "not base64!", "nightly").is_err());
    }

    #[test]
    fn finds_archive_sums() {
        let sums = std::str::from_utf8(SUMS).unwrap();
        assert_eq!(
            sum_for(sums, "horizon-windows-x64.zip"),
            Some("0e3157a74c56249573f42624d64bc87774324dec1cd20ba6518c784b7293eda0")
        );
        assert_eq!(sum_for(sums, "nope.zip"), None);
    }

    #[test]
    fn parses_nightly_from_refs() {
        let refs = "001e# service=git-upload-pack\n0000\
            00fa66f3981f522b24c250b21523a70080965b940424 HEAD\0multi_ack side-band-64k\n\
            003f66f3981f522b24c250b21523a70080965b940424 refs/tags/nightly\n\
            0052f319701e8c9ab657ff562250b3ba07b2d654e3bc refs/tags/nightly-source/2026-11-05-f319701\n\
            005266f3981f522b24c250b21523a70080965b940424 refs/tags/nightly-source/2026-11-05-66f3981\n";
        assert_eq!(
            parse_nightly_version(refs).as_deref(),
            Some("nightly-2026-11-05-66f3981")
        );
        assert_eq!(parse_nightly_version("0000"), None);
    }
}
