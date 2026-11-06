//! Live check of the official-launcher hand-off: get the launcher, write a profile.
use std::sync::Arc;

use elysium_core::http::Http;
use elysium_core::instance::InstanceManager;
use elysium_core::settings::Settings;
use elysium_core::{Paths, Progress, official};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths = Paths::from_env()?;
    let http = Http::new()?;
    let launcher = official::ensure_launcher(&paths, &http, Arc::new(Progress::new())).await?;
    println!(
        "launcher: {} ({} bytes)",
        launcher.display(),
        std::fs::metadata(&launcher)?.len()
    );
    let name = std::env::args().nth(1).ok_or("instance name")?;
    let inst = InstanceManager::new(paths.instances_dir())
        .list()?
        .into_iter()
        .find(|i| i.config.name == name)
        .ok_or("no instance")?;
    let key = official::write_profile(
        &paths,
        &inst,
        inst.version_id(),
        &Settings::load(&paths)?.effective(&inst.config.overrides),
    )?;
    println!("profile {key} for {}", inst.version_id());
    Ok(())
}
