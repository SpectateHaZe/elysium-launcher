//! Core of Elysium Launcher: Minecraft metadata, installation and launching.
//!
//! This crate is UI-agnostic; the desktop app drives it through async functions and
//! polls shared [`Progress`] values.

pub mod account;
pub mod assets;
pub mod error;
pub mod fsutil;
pub mod http;
pub mod install;
pub mod instance;
pub mod java;
pub mod launch;
pub mod logs;
pub mod meta;
pub mod paths;
pub mod progress;
pub mod rules;
pub mod settings;
pub mod versions;

pub use error::{Error, Result};
pub use paths::Paths;
pub use progress::Progress;
