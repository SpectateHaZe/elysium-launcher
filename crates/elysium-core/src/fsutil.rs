//! Small filesystem helpers.

use std::path::Path;

use serde::{Serialize, de::DeserializeOwned};

use crate::error::{Error, IoContext, Result};

/// Serialises `value` to `path` atomically (write to a sibling temp file, then rename),
/// so a crash never leaves a truncated config behind.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let data = serde_json::to_vec_pretty(value).map_err(|source| Error::Json {
        context: path.display().to_string(),
        source,
    })?;
    write_atomic(path, &data)
}

pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).at(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).at(&tmp)?;
    std::fs::rename(&tmp, path).at(path)
}

/// Reads JSON from `path`; `Ok(None)` if the file does not exist.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(data) => serde_json::from_slice(&data)
            .map(Some)
            .map_err(|source| Error::Json {
                context: path.display().to_string(),
                source,
            }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).at(path),
    }
}

/// Lower-case hex encoding.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// SHA-1 of a file, hex encoded.
pub fn sha1_file(path: &Path) -> Result<String> {
    use sha1::{Digest, Sha1};
    use std::io::Read;
    let mut file = std::fs::File::open(path).at(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).at(path)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encodes() {
        assert_eq!(hex(&[0x00, 0xab, 0x0f]), "00ab0f");
    }

    #[test]
    fn atomic_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b.json");
        write_json_atomic(&path, &vec![1, 2, 3]).unwrap();
        let back: Option<Vec<i32>> = read_json(&path).unwrap();
        assert_eq!(back, Some(vec![1, 2, 3]));
        assert!(
            read_json::<Vec<i32>>(&dir.path().join("missing.json"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn sha1_of_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha1_file(&path).unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }
}
