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
- **Automatic Java:** each version gets the exact Mojang Java runtime it was built for.
- **Instances** with their own memory, Java, JVM arguments and window size; live colour-coded game
  log; playtime tracking.
- **Apple-style themes:** light, dark or automatic; seven palettes plus a custom one built from any
  tint; Apple system accents or your own; text size and reduced transparency.

## Setup

- **Microsoft sign-in** needs an Azure application (client) ID that Mojang has approved for the
  Minecraft services API. Enter it in Settings, or build with `ELYSIUM_MSA_CLIENT_ID=<id>`.
- **CurseForge** needs an API key from the CurseForge for Studios console, entered in Settings.
  Modrinth works without any key.

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
