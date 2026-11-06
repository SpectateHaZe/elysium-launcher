# Elysium Launcher

A fast, modern Minecraft launcher written in Rust, with a Tauri desktop UI.

## Features

- **Every vanilla version**, from the newest snapshot back to `rd-132211`, including the old asset
  layouts and native libraries.
- **Mod loaders:** Fabric, Quilt, Forge and NeoForge, with every version they publish. Old Forge
  (1.12.2 and earlier) is installed natively; newer Forge and NeoForge run their official installer.
- **Discover:** search Modrinth and CurseForge together for mods, modpacks, resource packs, shaders
  and data packs, filtered to the instance's Minecraft version and loader. Required dependencies are
  installed too.
- **CurseForge fallback chain:** when an author disables third-party downloads, the identical file is
  fetched from Modrinth (matched by SHA-1); if it isn't there, the player downloads it from the
  CurseForge page and the launcher picks it up from the Downloads folder, checking the hash.
- **Modpacks:** install from Modrinth or CurseForge, import `.mrpack` and CurseForge zips, export any
  instance as a `.mrpack`.
- **Content manager:** enable, disable, remove and update installed mods, resource packs and shaders.
- **Accounts:** Microsoft sign-in (device code), skins (view and upload), and offline accounts.
  Refresh tokens are kept in the system credential store, never in plain files.
- **Horizon:** start [Horizon](https://github.com/horizon-rust/horizon), the Rust reimplementation of
  Java Edition, through its Launch API. Builds update automatically; every download is checked against
  Horizon's Ed25519-signed checksums before it is installed. Horizon requires a Microsoft account that
  owns Java Edition.
- **Play without signing in here:** like the CurseForge app, Elysium can hand the game to the
  official Minecraft Launcher, which signs you in. Microsoft sign-in in Elysium is optional, and
  offline accounts still work for singleplayer.
- **Crash helper:** explains crashes in plain words (missing mods, out of memory, wrong Java,
  driver problems, mod conflicts) with one-click fixes.
- **Import** instances from Prism Launcher, MultiMC, the CurseForge app and the official launcher.
- **World backups** before playing (once a day) with one-click restore; **Discord** presence.
- **Sensible defaults:** memory sized to your PC, tuned Java flags, and optional performance mods
  (Sodium, Lithium, FerriteCore and more) for new Fabric and Quilt instances.
- **Updates itself** from GitHub releases; every update is signed.
- **Automatic Java:** each version gets the exact Mojang Java runtime it was built for.
- **Instances** with their own memory, Java, JVM arguments and window size; live colour-coded game
  log; playtime tracking.
- **Apple-style themes:** light, dark or automatic; seven palettes plus a custom one built from any
  tint; Apple system accents or your own; text size and reduced transparency.

## Setup (repository secrets)

Releases are built by `.github/workflows/release.yml` on every push to `main`. Add these
repository secrets (Settings, Secrets and variables, Actions):

| Secret | What it does |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Signs updates so installed launchers accept them. Required for auto-updates. |
| `CURSEFORGE_API_KEY` | Built into the app so CurseForge works out of the box (apply at console.curseforge.com). |
| `MSA_CLIENT_ID` | Azure app ID approved by Mojang, for optional Microsoft sign-in in Elysium. |
| `DISCORD_APP_ID` | Discord application ID for Rich Presence (optional). |

Auto-updates download from this repository's releases, so the repository (or a separate
releases repository) must be public for installed launchers to see new versions.

## Layout

| Path | What it is |
| --- | --- |
| `crates/elysium-core` | UI-independent library: metadata, downloads, Java, install, launch |
| `app/src-tauri` | Tauri shell exposing the core to the UI as commands |
| `app/src` | Svelte 5 + TypeScript frontend |

## Building

Requirements: Rust (see `rust-version` in `Cargo.toml`), Node.js 22, and on Linux the
WebKitGTK development packages (`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libsoup-3.0-dev`).

```bash
cd app
npm ci
npx tauri dev      # run with hot reload
npx tauri build    # installers in target/release/bundle
```

Tests:

```bash
cargo test --workspace
```

`cargo run -p elysium-core --example launch -- 1.20.4 60` installs a version into a
throwaway instance and runs it for 60 seconds, which is useful for checking the install
pipeline without the UI. Set `ELYSIUM_HOME` to keep it away from your real data folder.

Data lives in `%APPDATA%\ElysiumLauncher` on Windows (or `$ELYSIUM_HOME` if set).

## Licence

Dual-licensed under MIT or Apache-2.0, at your option.
Bundled fonts and icons keep their own licences: Roboto Flex (SIL OFL 1.1) and
Material Symbols (Apache-2.0), shipped from their npm packages.
