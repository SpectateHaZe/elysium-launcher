//! Secret storage: the OS credential store (Windows Credential Manager, macOS Keychain,
//! Secret Service on Linux). When none is available, secrets live in memory for the
//! session only; they are never written to plain files.

use std::collections::HashMap;
use std::sync::Mutex;

const SERVICE: &str = "dev.elysium.launcher";

#[derive(Default)]
pub struct SecretStore {
    memory: Mutex<HashMap<String, String>>,
}

impl SecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a secret; returns `false` if it could only be kept in memory.
    pub fn set(&self, key: &str, value: &str) -> bool {
        self.memory
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.to_owned(), value.to_owned());
        match keyring::Entry::new(SERVICE, key).and_then(|e| e.set_password(value)) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("credential store unavailable, keeping secret in memory: {e}");
                false
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if let Some(v) = self
            .memory
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
        {
            return Some(v.clone());
        }
        keyring::Entry::new(SERVICE, key)
            .and_then(|e| e.get_password())
            .ok()
    }

    pub fn delete(&self, key: &str) {
        self.memory
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
        if let Ok(e) = keyring::Entry::new(SERVICE, key) {
            let _ = e.delete_credential();
        }
    }
}
