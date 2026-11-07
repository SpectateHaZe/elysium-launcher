//! Microsoft account sign-in for Minecraft: device-code OAuth, then Xbox Live, XSTS,
//! Minecraft services, the ownership check and the player profile.
//!
//! Needs an Azure application (client) ID that Mojang has approved for the Minecraft
//! services API.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::account::{Account, AccountKind, LaunchSession, Skin};
use crate::error::{Error, Result};
use crate::http::Http;

#[derive(Debug, Clone)]
pub struct AuthEndpoints {
    pub login: String,
    pub xbl: String,
    pub xsts: String,
    pub minecraft: String,
}

impl Default for AuthEndpoints {
    fn default() -> Self {
        Self {
            login: "https://login.microsoftonline.com/consumers/oauth2/v2.0".into(),
            xbl: "https://user.auth.xboxlive.com/user/authenticate".into(),
            xsts: "https://xsts.auth.xboxlive.com/xsts/authorize".into(),
            minecraft: "https://api.minecraftservices.com".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MicrosoftAuth {
    http: Http,
    client_id: String,
    endpoints: AuthEndpoints,
}

/// What to show the player while they sign in on another device or in the browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    #[serde(skip_serializing)]
    pub device_code: String,
}

/// Long-lived refresh token plus the current Minecraft token.
#[derive(Clone, Serialize, Deserialize)]
pub struct MicrosoftTokens {
    pub refresh_token: String,
    pub minecraft_token: String,
    pub expires_at: DateTime<Utc>,
    pub xuid: Option<String>,
}

impl std::fmt::Debug for MicrosoftTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicrosoftTokens")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct OAuthToken {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxToken {
    token: String,
    display_claims: Value,
}

fn xerr_message(code: u64) -> String {
    match code {
        2148916233 => "This Microsoft account has no Xbox profile yet. Sign in once at xbox.com, then try again.".into(),
        2148916235 => "Xbox Live isn't available in this account's country.".into(),
        2148916236 | 2148916237 => "This account needs adult verification on xbox.com first.".into(),
        2148916238 => "This is a child account. An adult needs to add it to a Microsoft family first.".into(),
        c => format!("Xbox Live refused the sign-in (error {c})."),
    }
}

impl MicrosoftAuth {
    pub fn new(http: Http, client_id: &str) -> Result<Self> {
        Self::with_endpoints(http, client_id, AuthEndpoints::default())
    }

    pub fn with_endpoints(http: Http, client_id: &str, endpoints: AuthEndpoints) -> Result<Self> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err(Error::Invalid(
                "Microsoft sign-in needs an Azure client ID; add one in Settings".into(),
            ));
        }
        Ok(Self {
            http,
            client_id: client_id.to_owned(),
            endpoints,
        })
    }

    pub async fn start(&self) -> Result<DeviceCode> {
        let resp = self
            .http
            .client()
            .post(format!("{}/devicecode", self.endpoints.login))
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", "XboxLive.signin offline_access"),
            ])
            .send()
            .await?;
        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            let desc = body
                .get("error_description")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(Error::Invalid(format!(
                "Microsoft sign-in couldn't start: {}",
                desc.lines().next().unwrap_or(desc)
            )));
        }
        Ok(resp.json().await?)
    }

    /// Polls once. `Ok(None)` while the player hasn't finished signing in yet.
    pub async fn poll(&self, code: &DeviceCode) -> Result<Option<(Account, MicrosoftTokens)>> {
        let resp = self
            .http
            .client()
            .post(format!("{}/token", self.endpoints.login))
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", self.client_id.as_str()),
                ("device_code", code.device_code.as_str()),
            ])
            .send()
            .await?;
        if resp.status().is_success() {
            let t: OAuthToken = resp.json().await?;
            // The code is spent now, so a failure from here on must not be retried as a
            // network hiccup: polling again only gets "already used" and hides the reason.
            return match self.finish(&t.access_token, t.refresh_token).await {
                Ok(done) => Ok(Some(done)),
                Err(Error::Http(e)) => Err(Error::Invalid(format!(
                    "Signed in to Microsoft, but Minecraft's servers didn't answer ({e}). Try again."
                ))),
                Err(e) => Err(e),
            };
        }
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        match body.get("error").and_then(Value::as_str) {
            Some("authorization_pending" | "slow_down") => Ok(None),
            Some("authorization_declined") => Err(Error::Invalid("Sign-in was cancelled.".into())),
            Some("expired_token") => Err(Error::Invalid(
                "The code expired. Start signing in again.".into(),
            )),
            other => {
                let desc = body.get("error_description").and_then(Value::as_str);
                Err(Error::Invalid(format!(
                    "Microsoft sign-in failed ({}). {}",
                    other.unwrap_or("unknown"),
                    desc.and_then(|d| d.lines().next()).unwrap_or_default()
                )))
            }
        }
    }

    /// Polls until the player finishes, the code expires, or `cancelled` returns true.
    pub async fn wait(
        &self,
        code: &DeviceCode,
        cancelled: impl Fn() -> bool,
    ) -> Result<(Account, MicrosoftTokens)> {
        let deadline = std::time::Instant::now() + Duration::from_secs(code.expires_in);
        let mut interval = code.interval.max(1);
        loop {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            if std::time::Instant::now() > deadline {
                return Err(Error::Invalid(
                    "The code expired. Start signing in again.".into(),
                ));
            }
            tokio::time::sleep(Duration::from_secs(interval)).await;
            match self.poll(code).await {
                Ok(Some(done)) => return Ok(done),
                Ok(None) => {}
                // Transient network trouble: back off and keep waiting.
                Err(Error::Http(_)) => interval = (interval + 2).min(15),
                Err(e) => return Err(e),
            }
        }
    }

    /// Exchanges a refresh token for fresh tokens and an up-to-date profile.
    pub async fn refresh(&self, refresh_token: &str) -> Result<(Account, MicrosoftTokens)> {
        let resp = self
            .http
            .client()
            .post(format!("{}/token", self.endpoints.login))
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", self.client_id.as_str()),
                ("refresh_token", refresh_token),
                ("scope", "XboxLive.signin offline_access"),
            ])
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::Invalid(
                "Your Microsoft sign-in expired. Sign in again.".into(),
            ));
        }
        let t: OAuthToken = resp.json().await?;
        self.finish(&t.access_token, t.refresh_token).await
    }

    async fn xbox(&self, url: &str, body: Value) -> Result<XboxToken> {
        let resp = self
            .http
            .client()
            .post(url)
            .header("accept", "application/json")
            .json(&body)
            .send()
            .await?;
        if resp.status().as_u16() == 401 {
            let err: Value = resp.json().await.unwrap_or(Value::Null);
            let code = err.get("XErr").and_then(Value::as_u64).unwrap_or(0);
            return Err(Error::Invalid(xerr_message(code)));
        }
        Ok(resp.error_for_status()?.json().await?)
    }

    async fn finish(
        &self,
        msa_token: &str,
        refresh_token: String,
    ) -> Result<(Account, MicrosoftTokens)> {
        let xbl = self
            .xbox(
                &self.endpoints.xbl,
                json!({"Properties": {"AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={msa_token}")},
                       "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT"}),
            )
            .await?;
        let xsts = self
            .xbox(
                &self.endpoints.xsts,
                json!({"Properties": {"SandboxId": "RETAIL", "UserTokens": [xbl.token]},
                       "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT"}),
            )
            .await?;
        let uhs = xsts
            .display_claims
            .pointer("/xui/0/uhs")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Invalid("Xbox Live returned no user hash".into()))?;
        let xuid = xsts
            .display_claims
            .pointer("/xui/0/xid")
            .and_then(Value::as_str)
            .map(str::to_owned);

        #[derive(Deserialize)]
        struct McLogin {
            access_token: String,
            expires_in: i64,
        }
        let mc = self
            .http
            .client()
            .post(format!(
                "{}/authentication/login_with_xbox",
                self.endpoints.minecraft
            ))
            .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{}", xsts.token) }))
            .send()
            .await?;
        // New Azure apps are refused here until Mojang approves them.
        if mc.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(Error::Invalid(
                "Minecraft hasn't approved this launcher's sign-in yet. Its Azure app needs Mojang's approval at aka.ms/mce-reviewappid.".into(),
            ));
        }
        let mc: McLogin = mc.error_for_status()?.json().await?;
        let tokens = MicrosoftTokens {
            refresh_token,
            expires_at: Utc::now() + chrono::Duration::seconds(mc.expires_in.max(60)),
            minecraft_token: mc.access_token,
            xuid,
        };
        let owns_game = self.owns_game(&tokens.minecraft_token).await?;
        let account = self.profile(&tokens.minecraft_token, owns_game).await?;
        Ok((account, tokens))
    }

    async fn owns_game(&self, mc_token: &str) -> Result<bool> {
        let items: Value = self
            .http
            .client()
            .get(format!("{}/entitlements/mcstore", self.endpoints.minecraft))
            .bearer_auth(mc_token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(items
            .get("items")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|i| {
                    matches!(
                        i.get("name").and_then(Value::as_str),
                        Some("product_minecraft" | "game_minecraft")
                    )
                })
            }))
    }

    /// The player's Minecraft profile (name, UUID, active skin).
    pub async fn profile(&self, mc_token: &str, owns_game: bool) -> Result<Account> {
        let resp = self
            .http
            .client()
            .get(format!("{}/minecraft/profile", self.endpoints.minecraft))
            .bearer_auth(mc_token)
            .send()
            .await?;
        if resp.status().as_u16() == 404 {
            return Err(Error::Invalid(if owns_game {
                "Choose a Minecraft name at minecraft.net first, then sign in again.".into()
            } else {
                "This Microsoft account doesn't own Minecraft: Java Edition.".into()
            }));
        }
        let p: Value = resp.error_for_status()?.json().await?;
        account_from_profile(&p, owns_game)
    }

    /// Uploads a new skin (PNG, 64×64 or 64×32) and returns the updated account.
    pub async fn upload_skin(
        &self,
        mc_token: &str,
        png: Vec<u8>,
        slim: bool,
        owns_game: bool,
    ) -> Result<Account> {
        if png.len() > 24 * 1024 || !png.starts_with(b"\x89PNG") {
            return Err(Error::Invalid(
                "Skins must be PNG files under 24 KB.".into(),
            ));
        }
        let form = reqwest::multipart::Form::new()
            .text("variant", if slim { "slim" } else { "classic" })
            .part(
                "file",
                reqwest::multipart::Part::bytes(png)
                    .file_name("skin.png")
                    .mime_str("image/png")?,
            );
        let p: Value = self
            .http
            .client()
            .post(format!(
                "{}/minecraft/profile/skins",
                self.endpoints.minecraft
            ))
            .bearer_auth(mc_token)
            .multipart(form)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        account_from_profile(&p, owns_game)
    }
}

fn account_from_profile(p: &Value, owns_game: bool) -> Result<Account> {
    let id = p
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid("profile has no id".into()))?;
    let uuid = Uuid::parse_str(id).map_err(|_| Error::Invalid(format!("bad profile id {id}")))?;
    let skin = p.get("skins").and_then(Value::as_array).and_then(|skins| {
        let s = skins
            .iter()
            .find(|s| s.get("state").and_then(Value::as_str) == Some("ACTIVE"))?;
        Some(Skin {
            url: s.get("url")?.as_str()?.to_owned(),
            variant: s
                .get("variant")
                .and_then(Value::as_str)
                .unwrap_or("CLASSIC")
                .to_lowercase(),
        })
    });
    Ok(Account {
        uuid,
        username: p
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        kind: AccountKind::Microsoft,
        owns_game,
        skin,
    })
}

impl MicrosoftTokens {
    /// Still valid for at least five minutes.
    pub fn is_fresh(&self) -> bool {
        self.expires_at - Utc::now() > chrono::Duration::minutes(5)
    }

    pub fn session(&self, account: &Account) -> LaunchSession {
        LaunchSession {
            username: account.username.clone(),
            uuid: account.uuid,
            access_token: self.minecraft_token.clone(),
            user_type: "msa",
            xuid: self.xuid.clone().unwrap_or_else(|| "0".into()),
            online: true,
        }
    }
}
