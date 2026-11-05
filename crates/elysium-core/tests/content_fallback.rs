//! The CurseForge → Modrinth → manual fallback chain, against a mock server.

use std::sync::Arc;

use elysium_core::Progress;
use elysium_core::content::curseforge::CurseForge;
use elysium_core::content::install::{ContentIndex, ContentService};
use elysium_core::content::modrinth::Modrinth;
use elysium_core::content::{ContentKind, Source};
use elysium_core::http::Http;
use elysium_core::instance::{InstanceManager, ModLoader};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const JAR: &[u8] = b"pretend this is a mod jar";
const UNKNOWN_SHA1: &str = "0000000000000000000000000000000000000000";

fn sha1_hex(data: &[u8]) -> String {
    use sha1::Digest;
    elysium_core::fsutil::hex(&sha1::Sha1::digest(data))
}

fn cf_file(
    id: u64,
    mod_id: u64,
    name: &str,
    sha1: &str,
    deps: serde_json::Value,
) -> serde_json::Value {
    json!({"id": id, "modId": mod_id, "displayName": name, "fileName": format!("{name}.jar"), "releaseType": 1,
           "fileDate": "2026-10-01T00:00:00Z", "fileLength": JAR.len(), "downloadUrl": null,
           "gameVersions": ["1.20.1", "Forge"], "hashes": [{"value": sha1, "algo": 1}], "dependencies": deps})
}

#[tokio::test]
async fn curseforge_blocked_files_fall_back_to_modrinth_then_manual() {
    let server = MockServer::start().await;
    let base = server.uri();
    let jar_sha1 = sha1_hex(JAR);

    // CurseForge: "Cool Mod" (100) requires "Library" (200). Neither allows direct downloads.
    Mock::given(method("GET"))
        .and(path("/v1/mods/100/files"))
        .and(header("x-api-key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            cf_file(1001, 100, "cool-mod", &jar_sha1, json!([{"modId": 200, "relationType": 3}]))
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/mods/200/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data": [cf_file(2001, 200, "library", UNKNOWN_SHA1, json!([]))]}),
        ))
        .mount(&server)
        .await;
    for (id, name, slug) in [(100, "Cool Mod", "cool-mod"), (200, "Library", "library")] {
        Mock::given(method("GET"))
            .and(path(format!("/v1/mods/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"data": {"id": id, "name": name, "slug": slug, "classId": 6}}),
            ))
            .mount(&server)
            .await;
    }

    // Modrinth has the identical Cool Mod file (same SHA-1), but not Library.
    let cdn_url = format!("{base}/cdn/cool-mod.jar");
    Mock::given(method("POST"))
        .and(path("/modrinth/version_files"))
        .respond_with(move |req: &Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
            let asked = body["hashes"][0].as_str().unwrap_or_default().to_owned();
            if asked == sha1_hex(JAR) {
                ResponseTemplate::new(200).set_body_json(json!({ asked.clone(): {
                    "id": "mr-v", "project_id": "mr-p", "name": "Cool Mod", "version_number": "1.0",
                    "version_type": "release", "date_published": "2026-10-01T00:00:00Z",
                    "files": [{"url": cdn_url, "filename": "cool-mod.jar", "primary": true, "hashes": {"sha1": asked}}]
                }}))
            } else {
                ResponseTemplate::new(200).set_body_json(json!({}))
            }
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/cdn/cool-mod.jar"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(JAR))
        .expect(1)
        .mount(&server)
        .await;

    let http = Http::new().unwrap();
    let service = ContentService::with_clients(
        http.clone(),
        Modrinth::with_base(http.clone(), &format!("{base}/modrinth")),
        CurseForge::with_base(http, &base, "test-key"),
    );
    let dir = tempfile::tempdir().unwrap();
    let inst = InstanceManager::new(dir.path())
        .create_with(
            "Forge pack",
            "1.20.1",
            ModLoader::Forge,
            Some("47.3.0".into()),
        )
        .unwrap();

    let report = service
        .install(
            &inst,
            Source::CurseForge,
            "100",
            None,
            ContentKind::Mod,
            None,
            Arc::new(Progress::new()),
        )
        .await
        .unwrap();

    // 1. Cool Mod came from Modrinth, verified by hash, recorded as the CurseForge project.
    assert_eq!(report.via_modrinth, ["Cool Mod"]);
    assert_eq!(report.installed.len(), 1);
    assert_eq!(
        std::fs::read(inst.game_dir().join("mods/cool-mod.jar")).unwrap(),
        JAR
    );
    let index = ContentIndex::load(&inst).unwrap();
    assert_eq!(index.entries[0].source, Some(Source::CurseForge));
    assert_eq!(index.entries[0].project_id.as_deref(), Some("100"));

    // 2. Library is nowhere else: the player gets a manual download for the right page.
    assert_eq!(report.manual.len(), 1);
    let m = &report.manual[0];
    assert_eq!(m.title, "Library");
    assert_eq!(
        m.page_url,
        "https://www.curseforge.com/minecraft/mc-mods/library/download/2001"
    );
    assert_eq!(m.entry.file, "mods/library.jar");
}
