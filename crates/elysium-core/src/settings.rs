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
    /// The custom theme's gradient: 1 to 5 `#rrggbb` colors, its angle in degrees, and
    /// how strongly it tints the window (0 to 100).
    pub custom_colors: Vec<String>,
    pub custom_angle: u16,
    pub custom_intensity: u8,
    /// A picture behind the custom theme (a copy in the data folder; empty for none) and
    /// how much to blur it, in pixels.
    pub background_image: String,
    pub background_blur: u8,
    pub text_size: TextSize,
    pub reduce_transparency: bool,
    /// Window background gradient: `none`, a preset id, or `custom`.
    pub gradient: String,
    /// `#rrggbb` start and end of the custom gradient.
    pub gradient_from: String,
    pub gradient_to: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            palette: "graphite".into(),
            accent: "palette".into(),
            custom_accent: "#007aff".into(),
            custom_colors: vec!["#ff8cc8".into(), "#a98bff".into()],
            custom_angle: 235,
            custom_intensity: 40,
            background_image: String::new(),
            background_blur: 16,
            text_size: TextSize::Default,
            reduce_transparency: false,
            gradient: "none".into(),
            gradient_from: "#5e5ce6".into(),
            gradient_to: "#ff2d55".into(),
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
        self.custom_colors.retain(|c| is_hex_color(c));
        self.custom_colors.truncate(5);
        if self.custom_colors.is_empty() {
            self.custom_colors = defaults.custom_colors;
        }
        self.custom_colors
            .iter_mut()
            .for_each(|c| c.make_ascii_lowercase());
        self.custom_angle %= 360;
        self.custom_intensity = self.custom_intensity.min(100);
        self.background_blur = self.background_blur.min(60);
        if !is_hex_color(&self.gradient_from) {
            self.gradient_from = defaults.gradient_from;
        }
        if !is_hex_color(&self.gradient_to) {
            self.gradient_to = defaults.gradient_to;
        }
        if self.gradient.is_empty() {
            self.gradient = defaults.gradient;
        }
        self.custom_accent.make_ascii_lowercase();
        self.gradient_from.make_ascii_lowercase();
        self.gradient_to.make_ascii_lowercase();
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
    /// The first-run welcome has been shown.
    pub onboarded: bool,
    /// Always hand games to the official Minecraft Launcher, even with an account here.
    pub always_official_launcher: bool,
    /// Add well-tested garbage collector flags for the game's Java.
    pub optimized_java_flags: bool,
    /// Back up an instance's worlds before playing (at most once a day).
    pub auto_backup: bool,
    /// Experimental: store identical mods and packs once across instances (hard links).
    pub share_files: bool,
    /// Ask before deleting an instance.
    pub confirm_delete: bool,
    /// Backups kept per world.
    pub backups_kept: u32,
    /// Show what you're playing in Discord.
    pub discord_presence: bool,
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
            onboarded: false,
            always_official_launcher: false,
            optimized_java_flags: true,
            auto_backup: false,
            share_files: false,
            confirm_delete: true,
            backups_kept: 5,
            discord_presence: true,
        }
    }
}

impl Settings {
    pub fn load(paths: &Paths) -> Result<Self> {
        let mut settings: Self = match fsutil::read_json(&paths.settings_file())? {
            Some(s) => s,
            // First run: size memory to this computer.
            None => {
                let (min, max) = recommended_memory(crate::system::total_memory_mb());
                Self {
                    min_memory_mb: min,
                    max_memory_mb: max,
                    ..Self::default()
                }
            }
        };
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
        let mut jvm_args = Vec::new();
        if self.optimized_java_flags {
            jvm_args.extend(optimized_flags(max));
        }
        // User arguments come last so they can override the preset.
        jvm_args.extend(split_args(o.jvm_args.as_deref().unwrap_or(&self.jvm_args)));
        EffectiveSettings {
            java_path: o.java_path.clone().or_else(|| self.java_path.clone()),
            min_memory_mb: min,
            max_memory_mb: max,
            jvm_args,
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

/// Memory for a computer with `total_mb` of RAM: half of it, between 2 and 8 GB,
/// leaving at least 2 GB for the system. Returns (min, max).
pub fn recommended_memory(total_mb: u64) -> (u32, u32) {
    if total_mb == 0 {
        return (512, 4096);
    }
    let half = total_mb / 2;
    let leave = total_mb.saturating_sub(2048);
    let max = half.min(leave).clamp(2048, 8192);
    // Round down to a 512 MB step so the slider shows a tidy value.
    let max = (max / 512 * 512) as u32;
    (512, max)
}

/// G1 garbage-collector tuning widely used for Minecraft (shorter, steadier pauses).
/// Supported by every Java the game runs on (8 and newer).
pub fn optimized_flags(max_memory_mb: u32) -> Vec<String> {
    let mut flags = vec![
        "-XX:+UseG1GC",
        "-XX:+ParallelRefProcEnabled",
        "-XX:MaxGCPauseMillis=200",
        "-XX:+UnlockExperimentalVMOptions",
        "-XX:+DisableExplicitGC",
        "-XX:+AlwaysPreTouch",
        "-XX:G1HeapWastePercent=5",
        "-XX:G1MixedGCCountTarget=4",
        "-XX:InitiatingHeapOccupancyPercent=15",
        "-XX:G1MixedGCLiveThresholdPercent=90",
        "-XX:G1RSetUpdatingPauseTimePercent=5",
        "-XX:SurvivorRatio=32",
        "-XX:+PerfDisableSharedMem",
        "-XX:MaxTenuringThreshold=1",
    ];
    // Bigger heaps want bigger regions and a larger young generation.
    if max_memory_mb >= 12 * 1024 {
        flags.extend([
            "-XX:G1NewSizePercent=40",
            "-XX:G1MaxNewSizePercent=50",
            "-XX:G1HeapRegionSize=16M",
            "-XX:G1ReservePercent=15",
        ]);
    } else {
        flags.extend([
            "-XX:G1NewSizePercent=30",
            "-XX:G1MaxNewSizePercent=40",
            "-XX:G1HeapRegionSize=8M",
            "-XX:G1ReservePercent=20",
        ]);
    }
    flags.into_iter().map(str::to_owned).collect()
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
            custom_colors: vec!["#ABCDEF".into(), "pink".into()],
            custom_angle: 400,
            palette: String::new(),
            gradient_to: "blue".into(),
            ..Default::default()
        };
        a.sanitize();
        assert_eq!(a.custom_accent, "#007aff");
        assert_eq!(a.custom_colors, ["#abcdef"]);
        assert_eq!(a.custom_angle, 40);
        assert_eq!(a.palette, "graphite");
        assert_eq!(a.gradient_to, "#ff2d55");
        assert!(is_hex_color("#00ff7A"));
        assert!(!is_hex_color("#00ff7"));
        assert!(!is_hex_color("00ff7a0"));
    }

    #[test]
    fn memory_recommendations() {
        assert_eq!(recommended_memory(4096), (512, 2048));
        assert_eq!(recommended_memory(8192), (512, 4096));
        assert_eq!(recommended_memory(16384), (512, 8192));
        assert_eq!(recommended_memory(65536), (512, 8192));
        assert_eq!(recommended_memory(12000), (512, 5632));
        assert_eq!(recommended_memory(0), (512, 4096));
    }

    #[test]
    fn optimized_flags_come_before_user_args() {
        let s = Settings {
            jvm_args: "-XX:MaxGCPauseMillis=50".into(),
            ..Default::default()
        };
        let e = s.effective(&InstanceOverrides::default());
        assert_eq!(e.jvm_args.first().map(String::as_str), Some("-XX:+UseG1GC"));
        assert_eq!(
            e.jvm_args.last().map(String::as_str),
            Some("-XX:MaxGCPauseMillis=50")
        );
        let off = Settings {
            optimized_java_flags: false,
            ..Default::default()
        }
        .effective(&InstanceOverrides::default());
        assert!(off.jvm_args.is_empty());
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
        assert_eq!(
            e.jvm_args.last().map(String::as_str),
            Some("-Dx=1"),
            "instance args replace global ones, after the preset"
        );
    }
}
