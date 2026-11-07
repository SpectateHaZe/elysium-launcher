//! Microsoft sign-in chain against a mock of every service involved.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use elysium_core::account::AccountKind;
use elysium_core::auth::{AuthEndpoints, MicrosoftAuth};
use elysium_core::http::Http;
use serde_json::json;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

async fn mount_chain(server: &MockServer, xsts_child: bool) {
    Mock::given(method("POST"))
        .and(path("/login/devicecode"))
        .and(body_string_contains("client_id=test-client"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_code": "ABCD-EFGH", "device_code": "dev-123", "verification_uri": "https://microsoft.com/link",
            "expires_in": 900, "interval": 1, "message": "go"})))
        .mount(server)
        .await;
    // First poll: still pending. Second: tokens.
    let polls = Arc::new(AtomicU32::new(0));
    Mock::given(method("POST"))
        .and(path("/login/token"))
        .respond_with(move |req: &Request| {
            let body = String::from_utf8_lossy(&req.body);
            if body.contains("grant_type=refresh_token") {
                return ResponseTemplate::new(200)
                    .set_body_json(json!({"access_token": "msa-2", "refresh_token": "refresh-2"}));
            }
            if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(400).set_body_json(json!({"error": "authorization_pending"}))
            } else {
                ResponseTemplate::new(200)
                    .set_body_json(json!({"access_token": "msa-1", "refresh_token": "refresh-1"}))
            }
        })
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/xbl"))
        .and(body_string_contains("d=msa-"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"Token": "xbl-token", "DisplayClaims": {"xui": [{"uhs": "hash1"}]}}),
        ))
        .mount(server)
        .await;
    let xsts = if xsts_child {
        ResponseTemplate::new(401).set_body_json(json!({"XErr": 2148916238u64}))
    } else {
        ResponseTemplate::new(200).set_body_json(json!({"Token": "xsts-token", "DisplayClaims": {"xui": [{"uhs": "hash1", "xid": "2535"}]}}))
    };
    Mock::given(method("POST"))
        .and(path("/xsts"))
        .respond_with(xsts)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/mc/authentication/login_with_xbox"))
        .and(body_string_contains("XBL3.0 x=hash1;xsts-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token": "mc-token", "expires_in": 86400})),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/mc/entitlements/mcstore"))
        .and(header("authorization", "Bearer mc-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"items": [{"name": "product_minecraft"}, {"name": "game_minecraft"}]}),
        ))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/mc/minecraft/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "069a79f444e94726a5befca90e38aaf5", "name": "Notch",
            "skins": [{"state": "ACTIVE", "url": "https://textures.minecraft.net/texture/abc", "variant": "SLIM"}]})))
        .mount(server)
        .await;
}

fn auth(server: &MockServer) -> MicrosoftAuth {
    let b = server.uri();
    MicrosoftAuth::with_endpoints(
        Http::new().unwrap(),
        "test-client",
        AuthEndpoints {
            login: format!("{b}/login"),
            xbl: format!("{b}/xbl"),
            xsts: format!("{b}/xsts"),
            minecraft: format!("{b}/mc"),
        },
    )
    .unwrap()
}

#[tokio::test]
async fn device_code_sign_in_and_refresh() {
    let server = MockServer::start().await;
    mount_chain(&server, false).await;
    let auth = auth(&server);

    let code = auth.start().await.unwrap();
    assert_eq!(code.user_code, "ABCD-EFGH");
    let (account, tokens) = auth.wait(&code, || false).await.unwrap();
    assert_eq!(account.username, "Notch");
    assert_eq!(
        account.uuid.simple().to_string(),
        "069a79f444e94726a5befca90e38aaf5"
    );
    assert_eq!(account.kind, AccountKind::Microsoft);
    assert!(account.owns_game);
    assert_eq!(account.skin.as_ref().unwrap().variant, "slim");
    assert_eq!(tokens.refresh_token, "refresh-1");
    assert!(tokens.is_fresh());
    let session = tokens.session(&account);
    assert_eq!(
        (session.user_type, session.xuid.as_str(), session.online),
        ("msa", "2535", true)
    );
    assert!(
        !format!("{session:?}").contains("mc-token"),
        "tokens never appear in debug output"
    );

    let (_, refreshed) = auth.refresh(&tokens.refresh_token).await.unwrap();
    assert_eq!(refreshed.refresh_token, "refresh-2");
}

#[tokio::test]
async fn child_account_gets_a_clear_message() {
    let server = MockServer::start().await;
    mount_chain(&server, true).await;
    let auth = auth(&server);
    let err = auth.refresh("refresh-1").await.unwrap_err().to_string();
    assert!(err.contains("child account"), "{err}");
}

#[tokio::test]
async fn unapproved_app_is_explained_not_retried() {
    let server = MockServer::start().await;
    // Wins over the chain's 200 for the same path.
    Mock::given(method("POST"))
        .and(path("/mc/authentication/login_with_xbox"))
        .respond_with(ResponseTemplate::new(403))
        .with_priority(1)
        .mount(&server)
        .await;
    mount_chain(&server, false).await;
    let auth = auth(&server);
    let code = auth.start().await.unwrap();
    let err = auth.wait(&code, || false).await.unwrap_err().to_string();
    assert!(err.contains("Mojang"), "{err}");
}

#[test]
fn missing_client_id_is_explained() {
    let err = MicrosoftAuth::new(Http::new().unwrap(), " ")
        .unwrap_err()
        .to_string();
    assert!(err.contains("client ID"), "{err}");
}
