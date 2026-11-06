//! Core of Elysium Launcher: Minecraft metadata, installation and launching.
//!
//! This crate is UI-agnostic; the desktop app drives it through async functions and
//! polls shared [`Progress`] values.

pub mod account;
pub mod assets;
pub mod auth;
pub mod backup;
pub mod content;
pub mod crash;
pub mod error;
pub mod fsutil;
pub mod gameopts;
pub mod horizon;
pub mod http;
pub mod import;
pub mod install;
pub mod instance;
pub mod java;
pub mod launch;
pub mod loaders;
pub mod logs;
pub mod meta;
pub mod official;
pub mod overview;
pub mod paths;
pub mod progress;
pub mod rules;
pub mod secrets;
pub mod settings;
pub mod share;
pub mod snapshot;
pub mod startup;
pub mod sync;
pub mod system;
pub mod versions;

pub use error::{Error, Result};
pub use paths::Paths;
pub use progress::Progress;
