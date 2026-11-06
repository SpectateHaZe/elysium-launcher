//! Live check of importing: scan other launchers, import one instance by name, time it.
//! `cargo run -p elysium-core --example import -- "<instance name>" [linked]`
use std::time::Instant;

use elysium_core::Paths;
use elysium_core::import;
use elysium_core::instance::InstanceManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths = Paths::from_env()?;
    let name = std::env::args().nth(1).ok_or("instance name")?;
    let t = Instant::now();
    let found = import::scan();
    println!("scan: {} instances in {:?}", found.len(), t.elapsed());
    let foreign = found
        .iter()
        .find(|f| f.name == name)
        .ok_or("no such instance")?;
    let t = Instant::now();
    let linked = std::env::args().nth(2).as_deref() == Some("linked");
    let inst = import::import(
        &InstanceManager::new(paths.instances_dir()),
        foreign,
        linked,
    )?;
    println!("import: {} in {:?}", inst.game_dir().display(), t.elapsed());
    Ok(())
}
