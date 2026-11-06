//! Discord Rich Presence: what you're playing, or what you're doing in Elysium when no
//! game is running. Without Discord running, this quietly does nothing.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

/// Elysium's Discord application (public, not a secret). A build can point at another.
const APP_ID: Option<&str> = match option_env!("ELYSIUM_DISCORD_APP_ID") {
    Some(id) => Some(id),
    None => Some("1568200385967951902"),
};

#[derive(Default)]
struct State {
    client: Option<DiscordIpcClient>,
    /// Games being played; browsing is only shown when this is zero.
    playing: usize,
    /// What the launcher window shows (details, state), shown between games.
    browsing: Option<(String, Option<String>)>,
    /// When the launcher started, for the "elapsed" timer while browsing.
    opened: i64,
}

#[derive(Default)]
pub struct Presence {
    state: Mutex<State>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Presence {
    pub fn available() -> bool {
        APP_ID.is_some_and(|id| !id.is_empty())
    }

    fn show(s: &mut State, activity: Activity<'_>) {
        let Some(id) = APP_ID.filter(|id| !id.is_empty()) else {
            return;
        };
        if s.client.is_none() {
            let mut client = DiscordIpcClient::new(id);
            if client.connect().is_err() {
                return;
            }
            s.client = Some(client);
        }
        if let Some(client) = s.client.as_mut()
            && client.set_activity(activity).is_err()
        {
            // Discord was closed; reconnect next time.
            s.client = None;
        }
    }

    fn show_browsing(s: &mut State) {
        let Some((details, state)) = s.browsing.clone() else {
            return;
        };
        if s.opened == 0 {
            s.opened = now();
        }
        let mut activity = Activity::new()
            .details(&details)
            .timestamps(Timestamps::new().start(s.opened))
            .assets(
                Assets::new()
                    .large_image("elysium")
                    .large_text("Elysium Launcher"),
            );
        if let Some(st) = &state {
            activity = activity.state(st);
        }
        Self::show(s, activity);
    }

    /// Shows the instance being played: its name, version line, and its own picture when
    /// it has one on the web (the Elysium logo otherwise, which then moves to the corner).
    pub fn playing(&self, details: &str, state: &str, image: Option<&str>, started: i64) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.playing += 1;
        let activity = Activity::new()
            .details(details)
            .state(state)
            .timestamps(Timestamps::new().start(started))
            .assets(match image {
                Some(url) => Assets::new()
                    .large_image(url)
                    .large_text(details)
                    .small_image("elysium")
                    .small_text("Elysium Launcher"),
                None => Assets::new()
                    .large_image("elysium")
                    .large_text("Elysium Launcher"),
            });
        Self::show(&mut s, activity);
    }

    /// A game ended: back to showing what the launcher is doing.
    pub fn clear(&self) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.playing = s.playing.saturating_sub(1);
        if s.playing > 0 {
            return;
        }
        if s.browsing.is_some() {
            Self::show_browsing(&mut s);
        } else if let Some(client) = s.client.as_mut() {
            let _ = client.clear_activity();
        }
    }

    /// What the launcher window shows right now. `None` hides the presence (turned off).
    pub fn browsing(&self, what: Option<(String, Option<String>)>) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let off = what.is_none();
        s.browsing = what;
        if s.playing > 0 {
            return;
        }
        if off {
            if let Some(client) = s.client.as_mut() {
                let _ = client.clear_activity();
            }
        } else {
            Self::show_browsing(&mut s);
        }
    }
}
