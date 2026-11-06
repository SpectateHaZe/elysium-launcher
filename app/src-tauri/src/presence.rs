//! Discord Rich Presence ("Playing Minecraft 1.21.1 with Fabric"). Needs a Discord
//! application ID built in at release time; without one, or without Discord running,
//! this quietly does nothing.

use std::sync::Mutex;

use discord_rich_presence::activity::{Activity, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

const APP_ID: Option<&str> = option_env!("ELYSIUM_DISCORD_APP_ID");

#[derive(Default)]
pub struct Presence {
    client: Mutex<Option<DiscordIpcClient>>,
}

impl Presence {
    pub fn available() -> bool {
        APP_ID.is_some_and(|id| !id.is_empty())
    }

    pub fn playing(&self, details: &str, state: &str, started: i64) {
        let Some(id) = APP_ID.filter(|id| !id.is_empty()) else {
            return;
        };
        let mut slot = self.client.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            let mut client = DiscordIpcClient::new(id);
            if client.connect().is_err() {
                return;
            }
            *slot = Some(client);
        }
        let activity = Activity::new()
            .details(details)
            .state(state)
            .timestamps(Timestamps::new().start(started))
            .assets(
                Assets::new()
                    .large_image("elysium")
                    .large_text("Elysium Launcher"),
            );
        if let Some(client) = slot.as_mut()
            && client.set_activity(activity).is_err()
        {
            // Discord was closed; reconnect next time.
            *slot = None;
        }
    }

    pub fn clear(&self) {
        let mut slot = self.client.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(client) = slot.as_mut() {
            let _ = client.clear_activity();
        }
    }
}
