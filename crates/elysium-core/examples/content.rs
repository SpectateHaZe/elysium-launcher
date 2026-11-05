//! Live check of content installs. Usage:
//!   content mod <instance-name> <modrinth-slug>
//!   content pack <modrinth-project> <game-version> <loader>

use std::sync::Arc;

use elysium_core::content::install::{self, ContentService};
use elysium_core::content::{ContentKind, Source, best_version};
use elysium_core::http::Http;
use elysium_core::instance::{InstanceManager, ModLoader};
use elysium_core::{Paths, Progress};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = Paths::from_env()?;
    let http = Http::new()?;
    let service = ContentService::new(http, "");
    let instances = InstanceManager::new(paths.instances_dir());
    match args.first().map(String::as_str) {
        Some("mod") => {
            let inst = instances
                .list()?
                .into_iter()
                .find(|i| i.config.name == args[1])
                .ok_or("no such instance")?;
            let report = service
                .install(
                    &inst,
                    Source::Modrinth,
                    &args[2],
                    None,
                    ContentKind::Mod,
                    None,
                    Arc::new(Progress::new()),
                )
                .await?;
            for e in &report.installed {
                println!(
                    "installed {} {} -> {}",
                    e.title,
                    e.version_number.as_deref().unwrap_or(""),
                    e.file
                );
            }
            println!(
                "skipped {:?} manual {}",
                report.skipped,
                report.manual.len()
            );
            for item in install::list(&inst)? {
                println!(
                    "  listed: {} ({}) enabled={}",
                    item.entry.title, item.entry.file, item.enabled
                );
            }
            println!("updates: {}", service.check_updates(&inst).await?.len());
        }
        Some("pack") => {
            let loader: ModLoader =
                serde_json::from_value(serde_json::Value::String(args[3].clone()))?;
            let versions = service
                .modrinth
                .versions(&args[1], ContentKind::Modpack, Some(&args[2]), Some(loader))
                .await?;
            let v = best_version(&versions).ok_or("no pack version")?;
            println!("pack version {} {}", v.name, v.id);
            let import = service
                .install_modrinth_pack(
                    &instances,
                    &paths.meta_dir(),
                    &args[1],
                    &v.id,
                    16,
                    Arc::new(Progress::new()),
                )
                .await?;
            let inst = instances.get(&import.instance_id)?;
            println!(
                "instance {} mc {} loader {:?} {:?}; {} files",
                inst.config.name,
                inst.config.game_version,
                inst.config.loader,
                inst.config.loader_version,
                install::list(&inst)?.len()
            );
        }
        _ => {
            return Err(
                "usage: content mod <instance> <slug> | content pack <project> <mc> <loader>"
                    .into(),
            );
        }
    }
    Ok(())
}
