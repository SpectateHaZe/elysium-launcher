//! Mods, modpacks, resource packs, shaders and data packs from Modrinth and CurseForge.

pub mod curseforge;
pub mod install;
pub mod local;
pub mod modpack;
pub mod modrinth;

use serde::{Deserialize, Serialize};

use crate::instance::ModLoader;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Modrinth,
    CurseForge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentKind {
    Mod,
    Modpack,
    ResourcePack,
    Shader,
    DataPack,
}

impl ContentKind {
    /// Folder inside the game directory (data packs go into a world's folder instead).
    pub fn folder(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::ResourcePack => "resourcepacks",
            Self::Shader => "shaderpacks",
            Self::DataPack => "datapacks",
            Self::Modpack => "",
        }
    }

    /// Whether files of this kind depend on the instance's mod loader.
    pub fn uses_loader(self) -> bool {
        matches!(self, Self::Mod | Self::Modpack)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Relevance,
    Downloads,
    Updated,
    Newest,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub kind: Option<ContentKind>,
    pub game_version: Option<String>,
    pub loader: Option<ModLoader>,
    pub sort: SortOrder,
    pub offset: u32,
    pub limit: u32,
}

/// One project in search results, from either source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectHit {
    pub source: Source,
    pub id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub author: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub updated: Option<String>,
    pub kind: ContentKind,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPage {
    pub hits: Vec<ProjectHit>,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileRef {
    /// `None` when the author doesn't allow third-party downloads (CurseForge).
    pub url: Option<String>,
    pub filename: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectVersion {
    pub source: Source,
    pub project_id: String,
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    /// `release`, `beta` or `alpha`.
    pub release_type: String,
    pub published: String,
    pub files: Vec<FileRef>,
    pub dependencies: Vec<Dependency>,
}

impl ProjectVersion {
    pub fn primary_file(&self) -> Option<&FileRef> {
        self.files.iter().find(|f| f.primary).or(self.files.first())
    }
}

/// Full project details for the detail sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetails {
    pub hit: ProjectHit,
    /// Long description (Markdown on Modrinth, HTML on CurseForge).
    pub body: String,
    pub body_format: String,
    pub website: Option<String>,
    pub gallery: Vec<String>,
}

/// Picks the newest version, preferring releases over betas and alphas.
pub fn best_version(versions: &[ProjectVersion]) -> Option<&ProjectVersion> {
    let rank = |v: &ProjectVersion| match v.release_type.as_str() {
        "release" => 0,
        "beta" => 1,
        _ => 2,
    };
    versions.iter().min_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| b.published.cmp(&a.published))
    })
}

/// Interleaves two result lists, CurseForge first. A project on both sites is shown once,
/// from CurseForge.
pub fn merge_results(modrinth: Vec<ProjectHit>, curseforge: Vec<ProjectHit>) -> Vec<ProjectHit> {
    let key = |h: &ProjectHit| {
        h.title
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    let seen: std::collections::HashSet<String> = curseforge.iter().map(key).collect();
    let modrinth: Vec<ProjectHit> = modrinth
        .into_iter()
        .filter(|h| !seen.contains(&key(h)))
        .collect();
    let mut out = Vec::with_capacity(modrinth.len() + curseforge.len());
    let (mut a, mut b) = (curseforge.into_iter(), modrinth.into_iter());
    loop {
        match (a.next(), b.next()) {
            (None, None) => break,
            (x, y) => out.extend(x.into_iter().chain(y)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(source: Source, title: &str) -> ProjectHit {
        ProjectHit {
            source,
            id: title.into(),
            slug: title.into(),
            title: title.into(),
            summary: String::new(),
            author: String::new(),
            icon_url: None,
            downloads: 0,
            updated: None,
            kind: ContentKind::Mod,
            categories: vec![],
        }
    }

    #[test]
    fn merges_and_dedupes() {
        let m = vec![
            hit(Source::Modrinth, "Sodium"),
            hit(Source::Modrinth, "Lithium"),
        ];
        let c = vec![
            hit(Source::CurseForge, "sodium"),
            hit(Source::CurseForge, "JEI"),
            hit(Source::CurseForge, "Create"),
        ];
        let merged = merge_results(m, c);
        let titles: Vec<_> = merged.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(titles, ["sodium", "Lithium", "JEI", "Create"]);
        assert_eq!(merged[0].source, Source::CurseForge);
    }

    #[test]
    fn prefers_releases() {
        let v = |id: &str, t: &str, d: &str| ProjectVersion {
            source: Source::Modrinth,
            project_id: "p".into(),
            id: id.into(),
            name: id.into(),
            version_number: id.into(),
            game_versions: vec![],
            loaders: vec![],
            release_type: t.into(),
            published: d.into(),
            files: vec![],
            dependencies: vec![],
        };
        let list = vec![
            v("a", "beta", "2026-03"),
            v("b", "release", "2026-01"),
            v("c", "release", "2026-02"),
        ];
        assert_eq!(best_version(&list).unwrap().id, "c");
    }
}
