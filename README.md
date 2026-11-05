# Elysium Launcher

A fast, modern Minecraft launcher written in Rust, with a Tauri desktop UI.

## Features

- Every vanilla version, from the current release and snapshots back to `rd-132211`,
  including the old asset layouts (`legacy`, `pre-1.6`) and legacy native libraries.
- Automatic Java: each version gets the exact Mojang Java runtime it was built for.
- Instances with their own game folder, memory, Java, JVM arguments and window size.
- Live, colour-coded game log (log4j XML output is parsed back into readable lines).
- Playtime and last-played tracking.
- Offline accounts. Microsoft sign-in is planned.
- Apple-style themes: light, dark or automatic; seven palettes plus a custom one built from
  any tint; Apple system accent colours or your own; text size and reduced transparency.

Planned: Microsoft accounts, Fabric/Quilt/Forge/NeoForge, Modrinth and CurseForge
search for mods, modpacks, shaders and resource packs, and modpack import/export.

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
