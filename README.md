<p align="center">
  <img src=".github/logo.png" alt="" width="120">
</p>

<h1 align="center">Elysium Launcher</h1>

<p align="center">A Minecraft launcher for Windows, written in Rust with a Tauri and Svelte front end.</p>

<p align="center"><a href="https://github.com/SpectateHaZe/elysium-launcher/releases/latest">Download the latest release</a></p>

## What it does

Elysium installs and runs any Minecraft: Java Edition version, from the newest snapshot back to
`rd-132211`, with Fabric, Quilt, Forge or NeoForge. Each instance has its own mods, memory, Java and
settings, and gets the Java runtime Mojang built that version for, downloaded on first launch.

You can search Modrinth and CurseForge from one place. Results are filtered to the instance's game
version and loader, and required dependencies come along. Some CurseForge authors block third-party
downloads; in that case Elysium looks for the identical file on Modrinth by its hash, and if it isn't
there, asks you to download it from CurseForge and picks it up from your Downloads folder.

Modpacks install from either site, or from a `.mrpack` or CurseForge zip. Any instance can be exported
as a `.mrpack`.

To play a modpack with friends, share it from the instance settings and send them the code. Your PC
serves the pack, and their copy follows yours: mods you add, update or remove are synced the next time
they press Play. Elysium tries to open the port on your router itself (UPnP). If that fails, friends on
the same network can still join.

Other things it handles:

- Microsoft and offline accounts, with skin upload. Sign-in tokens live in the system credential
  store.
- Playing through the official Minecraft Launcher instead, if you'd rather not sign in here.
- Crash reports explained in plain words, with a fix button for the common cases (missing mods,
  out of memory, wrong Java).
- Importing instances from Prism Launcher, MultiMC, the CurseForge app and the official launcher.
- A daily world backup before you play, with one-click restore.
- [Horizon](https://github.com/horizon-rust/horizon), the Rust reimplementation of Java Edition.
  Builds update automatically and are checked against Horizon's signed checksums.
- Discord Rich Presence, light and dark themes, custom palettes, and signed automatic updates.

Microsoft sign-in inside Elysium is waiting on Mojang's approval of the app. Until then, use offline
accounts or start the game through the official launcher.

## Building

You need Rust (version in `Cargo.toml`) and Node.js 22. On Linux, also install
`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libsoup-3.0-dev`.

```bash
cd app
npm ci
npx tauri dev
```

`npx tauri build` makes the installers. Run the tests with `cargo test --workspace`.

To check the install pipeline without the UI:

```bash
cargo run -p elysium-core --example launch -- 1.20.4 60
```

This installs 1.20.4 into a throwaway instance and runs it for 60 seconds. Set `ELYSIUM_HOME` to keep
it out of your real data folder, which is `%APPDATA%\ElysiumLauncher` on Windows.

The code is split in two. `crates/elysium-core` has everything that isn't UI: metadata, downloads,
Java, installing, launching, mods and sync. `app` is the Tauri shell (`app/src-tauri`) and the Svelte
front end (`app/src`).

## Releases

Every push to `main` builds and publishes a release through `.github/workflows/release.yml`. Two
repository secrets are used if present: `TAURI_SIGNING_PRIVATE_KEY` signs updates, and
`CURSEFORGE_API_KEY` builds in a CurseForge key. Without them the build still works, but without
auto-update files or built-in CurseForge search.

## Licence

MIT or Apache-2.0, at your option. Roboto Flex (SIL OFL 1.1) and Material Symbols (Apache-2.0) keep
their own licences.
