//! Headless end-to-end check: installs a version into a throwaway instance and runs it.
//!
//! Usage: `ELYSIUM_HOME=/tmp/elysium cargo run -p elysium-core --example launch -- <version> [seconds]`
//!
//! The game is killed after `seconds` (default 60). Exits non-zero if installation fails
//! or the game exits with an error before the timeout.

use std::sync::Arc;
use std::time::Duration;

use elysium_core::account::Account;
use elysium_core::http::{Http, Verify};
use elysium_core::install::{self, InstallOptions};
use elysium_core::instance::InstanceManager;
use elysium_core::launch;
use elysium_core::settings::Settings;
use elysium_core::{Paths, Progress};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let version = args.next().ok_or("usage: launch <version> [seconds]")?;
    let seconds: u64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(60);

    let paths = Paths::from_env()?;
    let http = Http::new()?;
    let instances = InstanceManager::new(paths.instances_dir());
    let instance = instances
        .list()?
        .into_iter()
        .find(|i| i.config.game_version == version)
        .map_or_else(
            || instances.create(&format!("test {version}"), &version),
            Ok,
        )?;

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
    let cmd = launch::build_command(&prepared, &instance, &account, &effective);
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
