//! Mojang metadata models and fetching.

pub mod manifest;
pub mod version;

pub use manifest::{ManifestEntry, VersionManifest, VersionType};
pub use version::{
    ArgValue, Argument, Arguments, Artifact, AssetIndexRef, JavaVersion, Library, VersionJson,
};
