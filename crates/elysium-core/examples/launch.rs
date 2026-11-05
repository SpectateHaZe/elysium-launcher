//! Headless end-to-end check: installs a version into a throwaway instance and runs it.
//!
//! Usage: `ELYSIUM_HOME=/tmp/elysium cargo run -p elysium-core --example launch -- <version> [seconds] [loader] [loader-version|latest]`
//!
//! The game is killed after `seconds` (default 60). Exits non-zero if installation fails
//! or the game exits with an error before the timeout.

use std::sync::Arc;
use std::time::Duration;

use elysium_core::account::{Account, LaunchSession};
use elysium_core::http::{Http, Verify};
use elysium_core::install::{self, InstallOptions};
use elysium_core::instance::{InstanceManager, ModLoader};
use elysium_core::launch;
use elysium_core::loaders;
use elysium_core::settings::Settings;
use elysium_core::{Paths, Progress};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let version = args.next().ok_or("usage: launch <version> [seconds]")?;
    let seconds: u64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(60);
    let loader: ModLoader = match args.next() {
        Some(l) => serde_json::from_value(serde_json::Value::String(l))?,
        None => ModLoader::Vanilla,
    };
    let loader_arg = args.next();

    let paths = Paths::from_env()?;
    let http = Http::new()?;
    let instances = InstanceManager::new(paths.instances_dir());
    let loader_version = match (loader, loader_arg.as_deref()) {
        (ModLoader::Vanilla, _) => None,
        (_, Some(v)) if v != "latest" => Some(v.to_owned()),
        _ => {
            let list = loaders::versions(&http, loader, &version).await?;
            let pick = list
                .iter()
                .find(|v| v.stable)
                .or(list.first())
                .ok_or("no loader versions")?;
            Some(pick.version.clone())
        }
    };
    println!("loader {loader:?} {loader_version:?}");
    let name = format!("test {version} {}", loader.name());
    let wanted = std::env::var("ELYSIUM_INSTANCE").ok();
    let instance = match instances.list()?.into_iter().find(|i| match &wanted {
        Some(w) => &i.config.name == w,
        None => i.config.name == name && i.config.loader_version == loader_version,
    }) {
        Some(i) => i,
        None => instances.create_with(&name, &version, loader, loader_version.clone())?,
    };

    let settings = Settings::default();
    let effective = settings.effective(&instance.config.overrides);
    let progress = Arc::new(Progress::new());
    let options = InstallOptions {
        java_path: effective.java_path.clone(),
        concurrency: settings.concurrent_downloads,
        verify: Verify::Size,
    };
    let started = std::time::Instant::now();
    let prepared =
        install::prepare(&paths, &http, &instance, &options, Arc::clone(&progress)).await?;
    let (done, _) = progress.counts();
    println!(
        "prepared in {:.1?}, downloaded {} KiB, java {}",
        started.elapsed(),
        done / 1024,
        prepared.java.display()
    );

    let account = Account::offline("ElysiumTest")?;
    let cmd = launch::build_command(
        &prepared,
        &instance,
        &LaunchSession::offline(&account),
        &effective,
    );
    let (kill_tx, kill_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        let _ = kill_tx.send(());
    });
    let exit = launch::run(
        &cmd,
        Arc::new(|l| println!("[{:?}] {}", l.level, l.text)),
        kill_rx,
    )
    .await?;
    println!("exit: {exit:?}");
    if exit.success() {
        Ok(())
    } else {
        Err(format!("game exited with {:?}", exit.code).into())
    }
}
