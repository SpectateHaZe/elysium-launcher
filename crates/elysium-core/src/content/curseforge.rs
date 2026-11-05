//! CurseForge API v1 client. Needs an API key from the CurseForge for Studios console.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ContentKind, Dependency, FileRef, ProjectDetails, ProjectHit, ProjectVersion, SearchPage,
    SearchQuery, SortOrder, Source,
};
use crate::error::{Error, Result};
use crate::http::Http;
use crate::instance::ModLoader;

pub const API: &str = "https://api.curseforge.com";
const MINECRAFT: u32 = 432;

pub fn class_id(kind: ContentKind) -> u32 {
    match kind {
        ContentKind::Mod => 6,
        ContentKind::Modpack => 4471,
        ContentKind::ResourcePack => 12,
        ContentKind::Shader => 6552,
        ContentKind::DataPack => 6945,
    }
}

pub fn kind_of(class_id: u32) -> ContentKind {
    match class_id {
        4471 => ContentKind::Modpack,
        12 => ContentKind::ResourcePack,
        6552 => ContentKind::Shader,
        6945 => ContentKind::DataPack,
        _ => ContentKind::Mod,
    }
}

/// URL segment of the CurseForge website for a kind (for manual download pages).
pub fn site_segment(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Mod => "mc-mods",
        ContentKind::Modpack => "modpacks",
        ContentKind::ResourcePack => "texture-packs",
        ContentKind::Shader => "shaders",
        ContentKind::DataPack => "data-packs",
    }
}

fn loader_type(loader: ModLoader) -> Option<u32> {
    match loader {
        ModLoader::Forge => Some(1),
        ModLoader::Fabric => Some(4),
        ModLoader::Quilt => Some(5),
        ModLoader::NeoForge => Some(6),
        ModLoader::Vanilla | ModLoader::Horizon => None,
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfMod {
    pub id: u64,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub download_count: f64,
    #[serde(default)]
    pub class_id: Option<u32>,
    #[serde(default)]
    pub logo: Option<Value>,
    #[serde(default)]
    pub authors: Vec<Value>,
    #[serde(default)]
    pub date_modified: Option<String>,
    #[serde(default)]
    pub links: Option<Value>,
    #[serde(default)]
    pub categories: Vec<Value>,
    #[serde(default)]
    pub screenshots: Vec<Value>,
}

impl CfMod {
    pub fn hit(&self) -> ProjectHit {
        ProjectHit {
            source: Source::CurseForge,
            id: self.id.to_string(),
            slug: self.slug.clone(),
            title: self.name.clone(),
            summary: self.summary.clone(),
            author: self
                .authors
                .first()
                .and_then(|a| a.get("name")?.as_str())
                .unwrap_or_default()
                .to_owned(),
            icon_url: self
                .logo
                .as_ref()
                .and_then(|l| l.get("thumbnailUrl")?.as_str())
                .map(str::to_owned),
            downloads: self.download_count as u64,
            updated: self.date_modified.clone(),
            kind: kind_of(self.class_id.unwrap_or(6)),
            categories: self
                .categories
                .iter()
                .filter_map(|c| c.get("name")?.as_str().map(str::to_owned))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfFile {
    pub id: u64,
    pub mod_id: u64,
    pub display_name: String,
    pub file_name: String,
    #[serde(default)]
    pub release_type: u8,
    #[serde(default)]
    pub file_date: String,
    #[serde(default)]
    pub file_length: Option<u64>,
    /// `null` when the author has turned off third-party downloads.
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub hashes: Vec<CfHash>,
    #[serde(default)]
    pub dependencies: Vec<CfDependency>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfHash {
    pub value: String,
    /// 1 = SHA-1, 2 = MD5.
    pub algo: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfDependency {
    pub mod_id: u64,
    /// 3 = required.
    pub relation_type: u8,
}

impl CfFile {
    pub fn sha1(&self) -> Option<&str> {
        self.hashes
            .iter()
            .find(|h| h.algo == 1)
            .map(|h| h.value.as_str())
    }

    pub fn version(&self) -> ProjectVersion {
        let loaders = ["Forge", "Fabric", "Quilt", "NeoForge"];
        ProjectVersion {
            source: Source::CurseForge,
            project_id: self.mod_id.to_string(),
            id: self.id.to_string(),
            name: self.display_name.clone(),
            version_number: self.display_name.clone(),
            game_versions: self
                .game_versions
                .iter()
                .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
                .cloned()
                .collect(),
            loaders: self
                .game_versions
                .iter()
                .filter(|v| loaders.contains(&v.as_str()))
                .map(|v| v.to_lowercase())
                .collect(),
            release_type: match self.release_type {
                1 => "release",
                2 => "beta",
                _ => "alpha",
            }
            .into(),
            published: self.file_date.clone(),
            files: vec![FileRef {
                url: self.download_url.clone(),
                filename: self.file_name.clone(),
                sha1: self.sha1().map(str::to_owned),
                size: self.file_length,
                primary: true,
            }],
            dependencies: self
                .dependencies
                .iter()
                .filter(|d| d.relation_type == 3 || d.relation_type == 2)
                .map(|d| Dependency {
                    project_id: Some(d.mod_id.to_string()),
                    version_id: None,
                    required: d.relation_type == 3,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CurseForge {
    http: Http,
    base: String,
    key: String,
}

#[derive(Deserialize)]
struct Data<T> {
    data: T,
}

impl CurseForge {
    /// `None` when no API key is configured.
    pub fn new(http: Http, key: &str) -> Option<Self> {
        Self::with_base(http, API, key)
    }

    pub fn with_base(http: Http, base: &str, key: &str) -> Option<Self> {
        let key = key.trim();
        (!key.is_empty()).then(|| Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            key: key.to_owned(),
        })
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<T> {
        let resp = req
            .header("x-api-key", &self.key)
            .header("accept", "application/json")
            .send()
            .await?;
        if resp.status().as_u16() == 403 || resp.status().as_u16() == 401 {
            return Err(Error::Invalid(
                "CurseForge rejected the API key; check it in Settings".into(),
            ));
        }
        resp.error_for_status()?.json().await.map_err(Error::from)
    }

    pub async fn search(&self, q: &SearchQuery) -> Result<SearchPage> {
        let kind = q.kind.unwrap_or(ContentKind::Mod);
        let mut query: Vec<(&str, String)> = vec![
            ("gameId", MINECRAFT.to_string()),
            ("classId", class_id(kind).to_string()),
            ("searchFilter", q.query.clone()),
            ("index", q.offset.to_string()),
            ("pageSize", q.limit.clamp(1, 50).to_string()),
            ("sortOrder", "desc".into()),
            (
                "sortField",
                match q.sort {
                    SortOrder::Relevance => "1",
                    SortOrder::Downloads => "6",
                    SortOrder::Updated => "3",
                    SortOrder::Newest => "11",
                }
                .into(),
            ),
        ];
        if let Some(v) = &q.game_version {
            query.push(("gameVersion", v.clone()));
        }
        if kind.uses_loader()
            && let Some(t) = q.loader.and_then(loader_type)
        {
            query.push(("modLoaderType", t.to_string()));
        }
        #[derive(Deserialize)]
        struct Page {
            data: Vec<CfMod>,
            pagination: Value,
        }
        let page: Page = self
            .send(
                self.http
                    .client()
                    .get(format!("{}/v1/mods/search", self.base))
                    .query(&query),
            )
            .await?;
        Ok(SearchPage {
            total: page
                .pagination
                .get("totalCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            hits: page.data.iter().map(CfMod::hit).collect(),
        })
    }

    pub async fn get_mod(&self, id: &str) -> Result<CfMod> {
        let d: Data<CfMod> = self
            .send(
                self.http
                    .client()
                    .get(format!("{}/v1/mods/{id}", self.base)),
            )
            .await?;
        Ok(d.data)
    }

    pub async fn mods(&self, ids: &[u64]) -> Result<Vec<CfMod>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let d: Data<Vec<CfMod>> = self
            .send(
                self.http
                    .client()
                    .post(format!("{}/v1/mods", self.base))
                    .json(&json!({ "modIds": ids })),
            )
            .await?;
        Ok(d.data)
    }

    pub async fn project(&self, id: &str) -> Result<ProjectDetails> {
        let m = self.get_mod(id).await?;
        let body: Data<String> = self
            .send(
                self.http
                    .client()
                    .get(format!("{}/v1/mods/{id}/description", self.base)),
            )
            .await
            .unwrap_or(Data {
                data: String::new(),
            });
        Ok(ProjectDetails {
            website: m
                .links
                .as_ref()
                .and_then(|l| l.get("websiteUrl")?.as_str())
                .map(str::to_owned),
            gallery: m
                .screenshots
                .iter()
                .filter_map(|s| s.get("url")?.as_str().map(str::to_owned))
                .collect(),
            hit: m.hit(),
            body: body.data,
            body_format: "html".into(),
        })
    }

    pub async fn files(
        &self,
        mod_id: &str,
        kind: ContentKind,
        game_version: Option<&str>,
        loader: Option<ModLoader>,
    ) -> Result<Vec<CfFile>> {
        let mut query: Vec<(&str, String)> = vec![("pageSize", "50".into())];
        if let Some(v) = game_version {
            query.push(("gameVersion", v.to_owned()));
        }
        if kind.uses_loader()
            && let Some(t) = loader.and_then(loader_type)
        {
            query.push(("modLoaderType", t.to_string()));
        }
        let d: Data<Vec<CfFile>> = self
            .send(
                self.http
                    .client()
                    .get(format!("{}/v1/mods/{mod_id}/files", self.base))
                    .query(&query),
            )
            .await?;
        Ok(d.data)
    }

    pub async fn files_by_id(&self, ids: &[u64]) -> Result<Vec<CfFile>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let d: Data<Vec<CfFile>> = self
            .send(
                self.http
                    .client()
                    .post(format!("{}/v1/mods/files", self.base))
                    .json(&json!({ "fileIds": ids })),
            )
            .await?;
        Ok(d.data)
    }

    pub async fn versions(
        &self,
        mod_id: &str,
        kind: ContentKind,
        game_version: Option<&str>,
        loader: Option<ModLoader>,
    ) -> Result<Vec<ProjectVersion>> {
        Ok(self
            .files(mod_id, kind, game_version, loader)
            .await?
            .iter()
            .map(CfFile::version)
            .collect())
    }
}

/// Website page where a player can download a file by hand.
pub fn manual_download_url(kind: ContentKind, slug: &str, file_id: &str) -> String {
    format!(
        "https://www.curseforge.com/minecraft/{}/{slug}/download/{file_id}",
        site_segment(kind)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_without_download_url() {
        let f: CfFile = serde_json::from_str(
            r#"{"id":5000001,"modId":238222,"displayName":"JEI 15.2","fileName":"jei-1.20.1-15.2.jar","releaseType":1,
                "fileDate":"2026-01-01T00:00:00Z","fileLength":1234,"downloadUrl":null,
                "gameVersions":["1.20.1","Forge","Client"],"hashes":[{"value":"abc","algo":1},{"value":"m","algo":2}],
                "dependencies":[{"modId":1,"relationType":3},{"modId":2,"relationType":1}]}"#,
        )
        .unwrap();
        let v = f.version();
        assert_eq!(v.game_versions, ["1.20.1"]);
        assert_eq!(v.loaders, ["forge"]);
        assert_eq!(v.files[0].url, None);
        assert_eq!(v.files[0].sha1.as_deref(), Some("abc"));
        assert_eq!(v.dependencies.len(), 1);
        assert_eq!(
            manual_download_url(ContentKind::Mod, "jei", "5000001"),
            "https://www.curseforge.com/minecraft/mc-mods/jei/download/5000001"
        );
    }

    #[test]
    fn no_key_no_client() {
        assert!(CurseForge::new(Http::new().unwrap(), "  ").is_none());
    }
}
