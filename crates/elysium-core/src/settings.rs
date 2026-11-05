//! Global launcher settings.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::fsutil;
use crate::instance::InstanceOverrides;
use crate::paths::Paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextSize {
    Small,
    #[default]
    Default,
    Large,
}

/// Look-and-feel choices. Palette and accent ids are interpreted by the UI; unknown ids
/// fall back to the defaults there, so old settings files keep working.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Theme palette id (`default`, `graphite`, `midnight`, … or `custom`).
    pub palette: String,
    /// Accent id: `palette` (the theme's own), an Apple system colour id, or `custom`.
    pub accent: String,
    /// `#rrggbb` used when `accent` is `custom`.
    pub custom_accent: String,
    /// `#rrggbb` the custom palette is derived from.
    pub custom_tint: String,
    pub text_size: TextSize,
    pub reduce_transparency: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            palette: "default".into(),
            accent: "palette".into(),
            custom_accent: "#007aff".into(),
            custom_tint: "#5e5ce6".into(),
            text_size: TextSize::Default,
            reduce_transparency: false,
        }
    }
}

impl Appearance {
    /// Replaces malformed colours with defaults so the UI never receives junk.
    pub fn sanitize(&mut self) {
        let defaults = Self::default();
        if !is_hex_color(&self.custom_accent) {
            self.custom_accent = defaults.custom_accent;
        }
        if !is_hex_color(&self.custom_tint) {
            self.custom_tint = defaults.custom_tint;
        }
        self.custom_accent.make_ascii_lowercase();
        self.custom_tint.make_ascii_lowercase();
        if self.palette.is_empty() {
            self.palette = defaults.palette;
        }
        if self.accent.is_empty() {
            self.accent = defaults.accent;
        }
    }
}

/// `#rrggbb`.
pub fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Custom Java for every instance; `None` uses the matching Mojang runtime.
    pub java_path: Option<PathBuf>,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    pub jvm_args: String,
    pub resolution: Option<Resolution>,
    pub concurrent_downloads: usize,
    /// Light/dark mode.
    pub theme: Theme,
    pub appearance: Appearance,
    /// Close the launcher window while the game is running.
    pub hide_on_launch: bool,
    /// Azure application (client) ID used for Microsoft sign-in.
    pub msa_client_id: String,
    /// Install new Horizon nightlies automatically.
    pub horizon_auto_update: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            java_path: None,
            min_memory_mb: 512,
            max_memory_mb: 4096,
            jvm_args: String::new(),
            resolution: None,
            concurrent_downloads: 16,
            theme: Theme::System,
            appearance: Appearance::default(),
            hide_on_launch: false,
            msa_client_id: option_env!("ELYSIUM_MSA_CLIENT_ID")
                .unwrap_or_default()
                .to_owned(),
            horizon_auto_update: true,
        }
    }
}

impl Settings {
    pub fn load(paths: &Paths) -> Result<Self> {
        let mut settings: Self = fsutil::read_json(&paths.settings_file())?.unwrap_or_default();
        settings.appearance.sanitize();
        Ok(settings)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        fsutil::write_json_atomic(&paths.settings_file(), self)
    }

    /// Combines global settings with an instance's overrides.
    pub fn effective(&self, o: &InstanceOverrides) -> EffectiveSettings {
        let max = o.max_memory_mb.unwrap_or(self.max_memory_mb).max(256);
        let min = o
            .min_memory_mb
            .unwrap_or(self.min_memory_mb)
            .clamp(128, max);
        EffectiveSettings {
            java_path: o.java_path.clone().or_else(|| self.java_path.clone()),
            min_memory_mb: min,
            max_memory_mb: max,
            jvm_args: split_args(o.jvm_args.as_deref().unwrap_or(&self.jvm_args)),
            resolution: o.resolution.or(self.resolution),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveSettings {
    pub java_path: Option<PathBuf>,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    pub jvm_args: Vec<String>,
    pub resolution: Option<Resolution>,
}

/// Splits a user-entered argument string on whitespace, honouring double quotes.
pub fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    for c in input.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_arguments() {
        assert_eq!(
            split_args(r#"  -XX:+UseG1GC "-Dpath=C:\My Folder" -Da= "" "#),
            vec!["-XX:+UseG1GC", r"-Dpath=C:\My Folder", "-Da=", ""]
        );
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn appearance_defaults_and_sanitizing() {
        // Files written before appearance settings existed still load.
        let old: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(old.appearance, Appearance::default());

        let mut a = Appearance {
            custom_accent: "red".into(),
            custom_tint: "#ABCDEF".into(),
            palette: String::new(),
            ..Default::default()
        };
        a.sanitize();
        assert_eq!(a.custom_accent, "#007aff");
        assert_eq!(a.custom_tint, "#abcdef");
        assert_eq!(a.palette, "default");
        assert!(is_hex_color("#00ff7A"));
        assert!(!is_hex_color("#00ff7"));
        assert!(!is_hex_color("00ff7a0"));
    }

    #[test]
    fn overrides_win_and_memory_is_clamped() {
        let s = Settings {
            max_memory_mb: 4096,
            min_memory_mb: 8192,
            ..Default::default()
        };
        let e = s.effective(&InstanceOverrides::default());
        assert_eq!((e.min_memory_mb, e.max_memory_mb), (4096, 4096));

        let o = InstanceOverrides {
            max_memory_mb: Some(2048),
            jvm_args: Some("-Dx=1".into()),
            ..Default::default()
        };
        let e = s.effective(&o);
        assert_eq!(e.max_memory_mb, 2048);
        assert_eq!(e.jvm_args, vec!["-Dx=1"]);
    }
}
