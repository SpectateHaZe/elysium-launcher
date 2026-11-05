//! Live check of Horizon's update path: discover, verify, download, unpack.

use std::sync::Arc;

use elysium_core::http::Http;
use elysium_core::{Paths, Progress, horizon};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths = Paths::from_env()?;
    let http = Http::new()?;
    let status = horizon::check(&paths, &http).await?;
    println!("status: {status:?}");
    let version = horizon::install_latest(&paths, &http, Arc::new(Progress::new())).await?;
    println!("installed {version}; state {:?}", horizon::state(&paths)?);
    let again = horizon::check(&paths, &http).await?;
    println!("after install: update_available={}", again.update_available);
    Ok(())
}
