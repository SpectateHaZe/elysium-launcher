//! Modrinth API v2 client.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ContentKind, Dependency, FileRef, ProjectDetails, ProjectHit, ProjectVersion, SearchPage,
    SearchQuery, SortOrder, Source,
};
use crate::error::{Error, Result};
use crate::http::Http;
use crate::instance::ModLoader;

pub const API: &str = "https://api.modrinth.com/v2";

/// Hosts a `.mrpack` may download from, per the Modrinth pack format.
pub const ALLOWED_PACK_HOSTS: &[&str] = &[
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "gitlab.com",
];

#[derive(Debug, Clone)]
pub struct Modrinth {
    http: Http,
    base: String,
}

fn project_type(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Mod => "mod",
        ContentKind::Modpack => "modpack",
        ContentKind::ResourcePack => "resourcepack",
        ContentKind::Shader => "shader",
        ContentKind::DataPack => "datapack",
    }
}

fn kind_of(project_type: &str) -> ContentKind {
    match project_type {
        "modpack" => ContentKind::Modpack,
        "resourcepack" => ContentKind::ResourcePack,
        "shader" => ContentKind::Shader,
        "datapack" => ContentKind::DataPack,
        _ => ContentKind::Mod,
    }
}

#[derive(Deserialize)]
struct RawHit {
    project_id: String,
    slug: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    date_modified: Option<String>,
    #[serde(default)]
    project_type: String,
    #[serde(default)]
    display_categories: Vec<String>,
}

#[derive(Deserialize)]
struct RawVersion {
    id: String,
    project_id: String,
    name: String,
    version_number: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    version_type: String,
    date_published: String,
    files: Vec<RawFile>,
    #[serde(default)]
    dependencies: Vec<RawDependency>,
}

#[derive(Deserialize)]
struct RawFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    hashes: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RawDependency {
    version_id: Option<String>,
    project_id: Option<String>,
    dependency_type: String,
}

impl From<RawVersion> for ProjectVersion {
    fn from(v: RawVersion) -> Self {
        ProjectVersion {
            source: Source::Modrinth,
            project_id: v.project_id,
            id: v.id,
            name: v.name,
            version_number: v.version_number,
            game_versions: v.game_versions,
            loaders: v.loaders,
            release_type: v.version_type,
            published: v.date_published,
            files: v
                .files
                .into_iter()
                .map(|f| FileRef {
                    url: Some(f.url),
                    sha1: f.hashes.get("sha1").cloned(),
                    filename: f.filename,
                    size: f.size,
                    primary: f.primary,
                })
                .collect(),
            dependencies: v
                .dependencies
                .into_iter()
                .filter(|d| d.dependency_type != "incompatible" && d.dependency_type != "embedded")
                .map(|d| Dependency {
                    required: d.dependency_type == "required",
                    project_id: d.project_id,
                    version_id: d.version_id,
                })
                .collect(),
        }
    }
}

impl From<RawHit> for ProjectHit {
    fn from(h: RawHit) -> Self {
        ProjectHit {
            source: Source::Modrinth,
            id: h.project_id,
            slug: h.slug,
            title: h.title,
            summary: h.description,
            author: h.author,
            icon_url: h.icon_url.filter(|u| !u.is_empty()),
            downloads: h.downloads,
            updated: h.date_modified,
            kind: kind_of(&h.project_type),
            categories: h.display_categories,
        }
    }
}

/// Modrinth's loader filter for a kind: mods use the instance loader, shaders and
/// packs have their own.
fn loader_facet(kind: ContentKind, loader: Option<ModLoader>) -> Option<String> {
    match kind {
        ContentKind::Mod | ContentKind::Modpack => {
            loader.and_then(ModLoader::modrinth_id).map(str::to_owned)
        }
        _ => None,
    }
}

impl Modrinth {
    pub fn new(http: Http) -> Self {
        Self::with_base(http, API)
    }

    pub fn with_base(http: Http, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let resp = self
            .http
            .client()
            .get(format!("{}{path}", self.base))
            .query(query)
            .send()
            .await?;
        let resp = resp.error_for_status()?;
        resp.json().await.map_err(Error::from)
    }

    async fn post<T: serde::de::DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let resp = self
            .http
            .client()
            .post(format!("{}{path}", self.base))
            .json(body)
            .send()
            .await?;
        resp.error_for_status()?.json().await.map_err(Error::from)
    }

    pub async fn search(&self, q: &SearchQuery) -> Result<SearchPage> {
        let kind = q.kind.unwrap_or(ContentKind::Mod);
        let mut facets: Vec<Vec<String>> =
            vec![vec![format!("project_type:{}", project_type(kind))]];
        if let Some(v) = &q.game_version {
            facets.push(vec![format!("versions:{v}")]);
        }
        if let Some(l) = loader_facet(kind, q.loader) {
            facets.push(vec![format!("categories:{l}")]);
        }
        let index = match q.sort {
            SortOrder::Relevance => "relevance",
            SortOrder::Downloads => "downloads",
            SortOrder::Updated => "updated",
            SortOrder::Newest => "newest",
        };
        #[derive(Deserialize)]
        struct Page {
            hits: Vec<RawHit>,
            total_hits: u64,
        }
        let page: Page = self
            .get(
                "/search",
                &[
                    ("query", q.query.clone()),
                    ("facets", serde_json::to_string(&facets).unwrap_or_default()),
                    ("index", index.to_owned()),
                    ("offset", q.offset.to_string()),
                    ("limit", q.limit.clamp(1, 100).to_string()),
                ],
            )
            .await?;
        Ok(SearchPage {
            hits: page.hits.into_iter().map(Into::into).collect(),
            total: page.total_hits,
        })
    }

    pub async fn project(&self, id: &str) -> Result<ProjectDetails> {
        #[derive(Deserialize)]
        struct Raw {
            id: String,
            slug: String,
            title: String,
            #[serde(default)]
            description: String,
            #[serde(default)]
            body: String,
            #[serde(default)]
            icon_url: Option<String>,
            #[serde(default)]
            downloads: u64,
            #[serde(default)]
            updated: Option<String>,
            #[serde(default)]
            project_type: String,
            #[serde(default)]
            categories: Vec<String>,
            #[serde(default)]
            gallery: Vec<Value>,
            #[serde(default)]
            team: String,
        }
        let p: Raw = self.get(&format!("/project/{id}"), &[]).await?;
        let website = Some(format!(
            "https://modrinth.com/{}/{}",
            if p.project_type.is_empty() {
                "project"
            } else {
                &p.project_type
            },
            p.slug
        ));
        Ok(ProjectDetails {
            gallery: p
                .gallery
                .iter()
                .filter_map(|g| g.get("url")?.as_str().map(str::to_owned))
                .collect(),
            hit: ProjectHit {
                source: Source::Modrinth,
                id: p.id,
                slug: p.slug,
                title: p.title,
                summary: p.description,
                author: p.team,
                icon_url: p.icon_url,
                downloads: p.downloads,
                updated: p.updated,
                kind: kind_of(&p.project_type),
                categories: p.categories,
            },
            body: p.body,
            body_format: "markdown".into(),
            website,
        })
    }

    /// Versions of a project, optionally filtered to a game version and loader.
    pub async fn versions(
        &self,
        project: &str,
        kind: ContentKind,
        game_version: Option<&str>,
        loader: Option<ModLoader>,
    ) -> Result<Vec<ProjectVersion>> {
        let mut query = Vec::new();
        if let Some(gv) = game_version {
            query.push(("game_versions", json!([gv]).to_string()));
        }
        if let Some(l) = loader_facet(kind, loader) {
            query.push(("loaders", json!([l]).to_string()));
        }
        let raw: Vec<RawVersion> = self
            .get(&format!("/project/{project}/version"), &query)
            .await?;
        Ok(raw.into_iter().map(Into::into).collect())
    }

    pub async fn version(&self, id: &str) -> Result<ProjectVersion> {
        let raw: RawVersion = self.get(&format!("/version/{id}"), &[]).await?;
        Ok(raw.into())
    }

    /// Looks files up by SHA-1; returns the versions Modrinth knows, keyed by hash.
    pub async fn versions_by_hash(
        &self,
        sha1s: &[String],
    ) -> Result<HashMap<String, ProjectVersion>> {
        if sha1s.is_empty() {
            return Ok(HashMap::new());
        }
        let raw: HashMap<String, RawVersion> = self
            .post(
                "/version_files",
                &json!({ "hashes": sha1s, "algorithm": "sha1" }),
            )
            .await?;
        Ok(raw.into_iter().map(|(k, v)| (k, v.into())).collect())
    }

    /// Newest compatible version for each file hash (only hashes that have one).
    pub async fn updates_by_hash(
        &self,
        sha1s: &[String],
        game_version: &str,
        loaders: &[&str],
    ) -> Result<HashMap<String, ProjectVersion>> {
        if sha1s.is_empty() {
            return Ok(HashMap::new());
        }
        let raw: HashMap<String, RawVersion> = self
            .post(
                "/version_files/update",
                &json!({ "hashes": sha1s, "algorithm": "sha1", "loaders": loaders, "game_versions": [game_version] }),
            )
            .await?;
        Ok(raw.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_json() {
        let raw: RawVersion = serde_json::from_str(
            r#"{"id":"v1","project_id":"p1","name":"Sodium 0.6","version_number":"0.6.0","game_versions":["1.21.1"],
                "loaders":["fabric"],"version_type":"release","date_published":"2026-01-01T00:00:00Z",
                "files":[{"url":"https://cdn.modrinth.com/x.jar","filename":"x.jar","primary":true,"size":10,"hashes":{"sha1":"aa","sha512":"bb"}}],
                "dependencies":[{"project_id":"fabric-api","dependency_type":"required"},{"project_id":"q","dependency_type":"incompatible"}]}"#,
        )
        .unwrap();
        let v: ProjectVersion = raw.into();
        assert_eq!(v.primary_file().unwrap().sha1.as_deref(), Some("aa"));
        assert_eq!(v.dependencies.len(), 1);
        assert!(v.dependencies[0].required);
    }

    #[test]
    fn loader_facets() {
        assert_eq!(
            loader_facet(ContentKind::Mod, Some(ModLoader::NeoForge)).as_deref(),
            Some("neoforge")
        );
        assert_eq!(
            loader_facet(ContentKind::Shader, Some(ModLoader::Fabric)),
            None
        );
        assert_eq!(
            loader_facet(ContentKind::Mod, Some(ModLoader::Vanilla)),
            None
        );
    }
}
