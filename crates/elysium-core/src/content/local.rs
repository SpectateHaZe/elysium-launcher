//! What a mod or pack file says about itself: its id, name, picture and code packages.
//! Read straight from the file, so it works for anything, wherever it came from.

use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ContentKind;
use crate::instance::Instance;

/// Pictures bigger than this are ignored.
const ICON_MAX: u64 = 2 * 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LocalMeta {
    /// Mod ids the file declares (a jar can hold several mods).
    pub ids: Vec<String>,
    pub name: Option<String>,
    pub icon: Option<Vec<u8>>,
}

type Zip = zip::ZipArchive<std::fs::File>;

fn open_zip(path: &Path) -> Option<Zip> {
    zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()
}

fn entry(zip: &mut Zip, name: &str, max: u64) -> Option<Vec<u8>> {
    let name = name.trim_start_matches('/');
    let f = zip.by_name(name).ok()?;
    if f.size() > max {
        return None;
    }
    let mut out = Vec::with_capacity(f.size() as usize);
    f.take(max).read_to_end(&mut out).ok()?;
    Some(out)
}

fn text(zip: &mut Zip, name: &str) -> Option<String> {
    String::from_utf8(entry(zip, name, 1024 * 1024)?).ok()
}

/// Fabric/Quilt icons are a path or a `{ "size": path }` map; take the biggest.
fn icon_path(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(m) => m
            .iter()
            .max_by_key(|(k, _)| k.parse::<u32>().unwrap_or(0))
            .and_then(|(_, p)| p.as_str().map(str::to_owned)),
        _ => None,
    }
}

fn toml_values(src: &str, key: &str) -> Vec<String> {
    let re = Regex::new(&format!(r#"(?m)^\s*{key}\s*=\s*["']([^"']+)["']"#)).expect("valid regex");
    re.captures_iter(src).map(|c| c[1].to_owned()).collect()
}

/// Reads a mod jar or a resource/shader pack (zip or folder).
pub fn read(path: &Path, kind: ContentKind) -> LocalMeta {
    if kind != ContentKind::Mod {
        let icon = if path.is_dir() {
            let p = path.join("pack.png");
            std::fs::metadata(&p)
                .ok()
                .filter(|m| m.len() <= ICON_MAX)
                .and_then(|_| std::fs::read(&p).ok())
        } else {
            open_zip(path).and_then(|mut z| entry(&mut z, "pack.png", ICON_MAX))
        };
        return LocalMeta {
            icon,
            ..LocalMeta::default()
        };
    }
    let Some(mut zip) = open_zip(path) else {
        return LocalMeta::default();
    };
    let mut meta = LocalMeta::default();
    let mut icon = None;
    if let Some(v) = text(&mut zip, "fabric.mod.json")
        .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
    {
        meta.ids.extend(v["id"].as_str().map(str::to_owned));
        meta.name = v["name"].as_str().map(str::to_owned);
        icon = icon_path(&v["icon"]);
    } else if let Some(v) =
        text(&mut zip, "quilt.mod.json").and_then(|t| serde_json::from_str::<Value>(&t).ok())
    {
        let q = &v["quilt_loader"];
        meta.ids.extend(q["id"].as_str().map(str::to_owned));
        meta.name = q["metadata"]["name"].as_str().map(str::to_owned);
        icon = icon_path(&q["metadata"]["icon"]);
    } else if let Some(t) = text(&mut zip, "META-INF/neoforge.mods.toml")
        .or_else(|| text(&mut zip, "META-INF/mods.toml"))
    {
        meta.ids = toml_values(&t, "modId");
        meta.name = toml_values(&t, "displayName").into_iter().next();
        icon = toml_values(&t, "logoFile").into_iter().next();
    } else if let Some(v) =
        text(&mut zip, "mcmod.info").and_then(|t| serde_json::from_str::<Value>(&t).ok())
    {
        // Old Forge: a list of mods, or `{ "modList": [...] }`.
        let list = v.as_array().or_else(|| v["modList"].as_array());
        if let Some(first) = list.and_then(|l| l.first()) {
            meta.ids.extend(first["modid"].as_str().map(str::to_owned));
            meta.name = first["name"].as_str().map(str::to_owned);
            icon = first["logoFile"].as_str().map(str::to_owned);
        }
    }
    meta.ids.retain(|i| !i.is_empty() && !i.contains("${"));
    meta.name = meta
        .name
        .filter(|n| !n.trim().is_empty() && !n.contains("${"));
    meta.icon = icon
        .filter(|p| !p.is_empty())
        .and_then(|p| entry(&mut zip, &p, ICON_MAX));
    meta
}

/// The Java packages a jar's own classes live in (bundled jars inside it aren't opened),
/// cut to at most three levels: `net/caffeinemc/mods/sodium/X.class` gives
/// `net.caffeinemc.mods`.
pub fn packages(path: &Path) -> Vec<String> {
    let Some(zip) = open_zip(path) else {
        return Vec::new();
    };
    let mut out = BTreeSet::new();
    for name in zip.file_names().filter_map(Result::ok) {
        if !name.ends_with(".class") || name.starts_with("META-INF/") {
            continue;
        }
        let dirs: Vec<&str> = name.split('/').collect();
        let dirs = &dirs[..dirs.len() - 1];
        if dirs.is_empty() {
            continue;
        }
        out.insert(dirs[..dirs.len().min(3)].join("."));
    }
    out.into_iter().collect()
}

/// An enabled mod in an instance, for naming culprits in crashes and slow starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModInfo {
    /// `mods/<file>`.
    pub file: String,
    pub ids: Vec<String>,
    pub name: String,
    pub packages: Vec<String>,
}

/// Packages shared by Minecraft, the loaders and common libraries: never evidence
/// against a mod that happens to include them.
const COMMON: &[&str] = &[
    "java.",
    "javax.",
    "jdk.",
    "sun.",
    "com.sun.",
    "net.minecraft",
    "com.mojang",
    "org.spongepowered",
    "net.fabricmc",
    "org.quiltmc",
    "net.neoforged",
    "net.minecraftforge",
    "cpw.",
    "org.lwjgl",
    "io.netty",
    "com.google",
    "org.apache",
    "it.unimi",
    "org.slf4j",
    "kotlin",
    "org.objectweb",
    "com.llamalad7",
    "org.jetbrains",
    "dev.architectury",
];

pub fn scan_mods(instance: &Instance) -> Vec<ModInfo> {
    let dir = instance.game_dir().join("mods");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".jar") {
            continue;
        }
        let path = e.path();
        let meta = read(&path, ContentKind::Mod);
        out.push(ModInfo {
            file: format!("mods/{name}"),
            name: meta
                .name
                .clone()
                .unwrap_or_else(|| name.trim_end_matches(".jar").to_owned()),
            ids: meta.ids,
            packages: packages(&path)
                .into_iter()
                .filter(|p| !COMMON.iter().any(|c| p.starts_with(c)))
                .collect(),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

impl ModInfo {
    /// The mod a fully qualified class name belongs to, by the longest package match.
    pub fn owner<'a>(mods: &'a [ModInfo], class: &str) -> Option<&'a ModInfo> {
        if COMMON.iter().any(|c| class.starts_with(c)) {
            return None;
        }
        mods.iter()
            .filter_map(|m| {
                m.packages
                    .iter()
                    .filter(|p| class.starts_with(p.as_str()) && class[p.len()..].starts_with('.'))
                    .map(|p| p.len())
                    .max()
                    .map(|len| (len, m))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, m)| m)
    }

    /// The mod with this id or name (case-insensitive).
    pub fn by_id<'a>(mods: &'a [ModInfo], id: &str) -> Option<&'a ModInfo> {
        let id = id.trim();
        mods.iter().find(|m| {
            m.ids.iter().any(|i| i.eq_ignore_ascii_case(id)) || m.name.eq_ignore_ascii_case(id)
        })
    }
}

// ---------------------------------------------------------------------------
// Picture and name cache for the content list.

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    entries: HashMap<String, Cached>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cached {
    name: Option<String>,
    icon: bool,
}

/// Looks up (and remembers) the name and picture of local files, keyed by name, size and
/// modification time so a changed file is read again.
pub struct IconCache {
    dir: PathBuf,
    cache: Cache,
    seen: Vec<String>,
    dirty: bool,
}

impl IconCache {
    pub fn open(instance: &Instance) -> Self {
        let dir = instance.dir.join("content-icons");
        let cache = std::fs::read(dir.join("index.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            dir,
            cache,
            seen: Vec::new(),
            dirty: false,
        }
    }

    /// Name the file gives itself and the path of its cached picture.
    pub fn get(
        &mut self,
        path: &Path,
        kind: ContentKind,
        meta: &std::fs::Metadata,
    ) -> (Option<String>, Option<PathBuf>) {
        use sha1::Digest;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis());
        let key = crate::fsutil::hex(&sha1::Sha1::digest(format!(
            "{}|{}|{mtime}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            meta.len()
        )));
        let icon_file = self.dir.join(format!("{key}.png"));
        self.seen.push(key.clone());
        let cached = match self.cache.entries.get(&key) {
            Some(c) if !c.icon || icon_file.is_file() => c.clone(),
            _ => {
                let m = read(path, kind);
                let icon = m.icon.is_some_and(|bytes| {
                    std::fs::create_dir_all(&self.dir).is_ok()
                        && std::fs::write(&icon_file, bytes).is_ok()
                });
                let c = Cached { name: m.name, icon };
                self.cache.entries.insert(key, c.clone());
                self.dirty = true;
                c
            }
        };
        (cached.name, cached.icon.then_some(icon_file))
    }

    /// Saves the index and drops pictures of files that are gone.
    pub fn finish(mut self) {
        let before = self.cache.entries.len();
        self.cache.entries.retain(|k, _| self.seen.contains(k));
        if self.cache.entries.len() != before {
            self.dirty = true;
            if let Ok(read) = std::fs::read_dir(&self.dir) {
                for e in read.flatten() {
                    let n = e.file_name().to_string_lossy().into_owned();
                    if let Some(k) = n.strip_suffix(".png")
                        && !self.cache.entries.contains_key(k)
                    {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        }
        if self.dirty {
            let _ = std::fs::create_dir_all(&self.dir);
            if let Err(e) =
                crate::fsutil::write_json_atomic(&self.dir.join("index.json"), &self.cache)
            {
                tracing::warn!("couldn't save content picture cache: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn jar(path: &Path, files: &[(&str, &[u8])]) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (n, b) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(b).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn reads_fabric_neoforge_and_packs() {
        let dir = tempfile::tempdir().unwrap();
        let fabric = dir.path().join("sodium.jar");
        jar(
            &fabric,
            &[
                (
                    "fabric.mod.json",
                    br#"{"id":"sodium","name":"Sodium","icon":{"32":"a/s.png","128":"a/l.png"}}"#,
                ),
                ("a/l.png", b"big"),
                ("a/s.png", b"small"),
                ("net/caffeinemc/mods/sodium/client/X.class", b""),
                ("net/minecraft/client/Mixin.class", b""),
            ],
        );
        let m = read(&fabric, ContentKind::Mod);
        assert_eq!(m.ids, ["sodium"]);
        assert_eq!(m.name.as_deref(), Some("Sodium"));
        assert_eq!(m.icon.as_deref(), Some(&b"big"[..]));
        assert_eq!(
            packages(&fabric),
            ["net.caffeinemc.mods", "net.minecraft.client"]
        );

        let neo = dir.path().join("jei.jar");
        jar(
            &neo,
            &[
                (
                    "META-INF/neoforge.mods.toml",
                    b"logoFile=\"logo.png\"\n[[mods]]\nmodId=\"jei\"\ndisplayName=\"Just Enough Items\"\n",
                ),
                ("logo.png", b"jei"),
            ],
        );
        let m = read(&neo, ContentKind::Mod);
        assert_eq!(
            (m.ids, m.name.as_deref(), m.icon.as_deref()),
            (
                vec!["jei".to_owned()],
                Some("Just Enough Items"),
                Some(&b"jei"[..])
            )
        );

        let folder = dir.path().join("Template");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("pack.png"), b"p").unwrap();
        assert_eq!(
            read(&folder, ContentKind::ResourcePack).icon.as_deref(),
            Some(&b"p"[..])
        );
        assert_eq!(
            read(&dir.path().join("nope.jar"), ContentKind::Mod),
            LocalMeta::default()
        );
    }

    #[test]
    fn owner_takes_longest_package_and_ignores_minecraft() {
        let m = |name: &str, pkgs: &[&str]| ModInfo {
            file: format!("mods/{name}.jar"),
            ids: vec![name.to_owned()],
            name: name.to_owned(),
            packages: pkgs.iter().map(|s| (*s).to_owned()).collect(),
        };
        let mods = [m("a", &["com.example"]), m("b", &["com.example.b"])];
        assert_eq!(
            ModInfo::owner(&mods, "com.example.b.Foo").unwrap().name,
            "b"
        );
        assert_eq!(ModInfo::owner(&mods, "com.example.Foo").unwrap().name, "a");
        assert!(ModInfo::owner(&mods, "com.examples.Foo").is_none());
        assert!(ModInfo::owner(&mods, "net.minecraft.Foo").is_none());
        assert_eq!(ModInfo::by_id(&mods, "B").unwrap().name, "b");
    }
}
