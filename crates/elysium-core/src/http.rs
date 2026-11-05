//! HTTP client and verified, concurrent file downloads.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::{StreamExt, TryStreamExt};
use serde::de::DeserializeOwned;
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

use crate::error::{Error, IoContext, Result};
use crate::fsutil::hex;
use crate::progress::Progress;

const USER_AGENT: &str = concat!("ElysiumLauncher/", env!("CARGO_PKG_VERSION"));
const RETRIES: u32 = 3;

/// A file to download and where to put it.
#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    /// Mark executable after download (Unix only; Java runtime binaries).
    pub executable: bool,
}

impl DownloadTask {
    pub fn new(url: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            path: path.into(),
            sha1: None,
            size: None,
            executable: false,
        }
    }

    pub fn sha1(mut self, sha1: Option<impl Into<String>>) -> Self {
        self.sha1 = sha1.map(Into::into);
        self
    }

    pub fn size(mut self, size: Option<u64>) -> Self {
        self.size = size;
        self
    }
}

/// How thoroughly existing files are checked before deciding to skip them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verify {
    /// Skip a file if it exists with the expected size. Fast; used on normal launches.
    Size,
    /// Re-hash every existing file. Used by "repair".
    Hash,
}

#[derive(Debug, Clone)]
pub struct Http {
    client: reqwest::Client,
}

impl Http {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self { client })
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let bytes = self.get_bytes(url).await?;
        serde_json::from_slice(&bytes).map_err(|source| Error::Json {
            context: url.to_owned(),
            source,
        })
    }

    pub async fn get_bytes(&self, url: &str) -> Result<bytes::Bytes> {
        let mut attempt = 0;
        loop {
            let result = async {
                let resp = self.client.get(url).send().await?.error_for_status()?;
                resp.bytes().await
            }
            .await;
            match result {
                Ok(b) => return Ok(b),
                Err(e) if attempt + 1 < RETRIES && is_transient(&e) => {
                    attempt += 1;
                    tokio::time::sleep(backoff(attempt)).await;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Downloads one file, verifying its SHA-1 when known. Returns bytes transferred.
    pub async fn download(&self, task: &DownloadTask, progress: Option<&Progress>) -> Result<u64> {
        let mut attempt = 0;
        loop {
            match self.download_once(task, progress).await {
                Ok(n) => return Ok(n),
                Err(Error::Http(e)) if attempt + 1 < RETRIES && is_transient(&e) => {
                    attempt += 1;
                    tracing::warn!(url = %task.url, attempt, "download failed, retrying: {e}");
                    tokio::time::sleep(backoff(attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn download_once(&self, task: &DownloadTask, progress: Option<&Progress>) -> Result<u64> {
        if let Some(parent) = task.path.parent() {
            tokio::fs::create_dir_all(parent).await.at(parent)?;
        }
        let part = part_path(&task.path);
        let resp = self
            .client
            .get(&task.url)
            .send()
            .await?
            .error_for_status()?;
        let mut file = tokio::fs::File::create(&part).await.at(&part)?;
        let mut hasher = Sha1::new();
        let mut written = 0u64;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if let Some(p) = progress {
                p.check()?;
            }
            let chunk = chunk?;
            hasher.update(&chunk);
            file.write_all(&chunk).await.at(&part)?;
            written += chunk.len() as u64;
            if let Some(p) = progress {
                p.advance(chunk.len() as u64);
            }
        }
        file.flush().await.at(&part)?;
        drop(file);

        let actual = hex(&hasher.finalize());
        if let Some(expected) = &task.sha1
            && !expected.eq_ignore_ascii_case(&actual)
        {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::Checksum {
                url: task.url.clone(),
                expected: expected.clone(),
                actual,
            });
        }
        tokio::fs::rename(&part, &task.path).await.at(&task.path)?;
        if task.executable {
            set_executable(&task.path)?;
        }
        Ok(written)
    }

    /// Downloads every task that is missing or invalid, `concurrency` at a time.
    ///
    /// Progress is reported in bytes; the stage total is extended by the size of the
    /// files that actually need downloading.
    pub async fn download_all(
        &self,
        tasks: Vec<DownloadTask>,
        concurrency: usize,
        verify: Verify,
        progress: Arc<Progress>,
    ) -> Result<()> {
        let pending = filter_pending(tasks, verify).await?;
        if pending.is_empty() {
            return Ok(());
        }
        progress.add_total(pending.iter().map(|t| t.size.unwrap_or(0)).sum());
        futures::stream::iter(pending.into_iter().map(Ok))
            .try_for_each_concurrent(concurrency.max(1), |task| {
                let progress = Arc::clone(&progress);
                async move {
                    progress.check()?;
                    self.download(&task, Some(&progress)).await.map(|_| ())
                }
            })
            .await
    }
}

/// Returns the tasks whose target is missing or fails verification.
async fn filter_pending(tasks: Vec<DownloadTask>, verify: Verify) -> Result<Vec<DownloadTask>> {
    tokio::task::spawn_blocking(move || {
        let mut seen = std::collections::HashSet::new();
        let mut pending = Vec::new();
        for task in tasks {
            if !seen.insert(task.path.clone()) {
                continue;
            }
            if !is_valid(&task, verify)? {
                pending.push(task);
            }
        }
        Ok(pending)
    })
    .await
    .map_err(|e| Error::Invalid(format!("verification task failed: {e}")))?
}

fn is_valid(task: &DownloadTask, verify: Verify) -> Result<bool> {
    let meta = match std::fs::metadata(&task.path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).at(&task.path),
    };
    if task.size.is_some_and(|s| s != meta.len()) {
        return Ok(false);
    }
    match (verify, &task.sha1) {
        (Verify::Hash, Some(expected)) => {
            Ok(crate::fsutil::sha1_file(&task.path)?.eq_ignore_ascii_case(expected))
        }
        _ => Ok(true),
    }
}

fn part_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    path.with_file_name(name)
}

fn is_transient(e: &reqwest::Error) -> bool {
    e.is_timeout()
        || e.is_connect()
        || e.is_body()
        || e.is_request()
        || e.status()
            .is_some_and(|s| s.is_server_error() || s.as_u16() == 429)
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500 * 2u64.pow(attempt))
}

#[cfg(unix)]
pub(crate) fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).at(path)?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms).at(path)
}

#[cfg(not(unix))]
pub(crate) fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_path_appends_suffix() {
        assert_eq!(
            part_path(Path::new("/a/b.jar")),
            PathBuf::from("/a/b.jar.part")
        );
    }

    #[tokio::test]
    async fn filters_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good");
        std::fs::write(&good, b"abc").unwrap();
        let sha = "a9993e364706816aba3e25717850c26c9cd0d89d";
        let tasks = vec![
            DownloadTask::new("u1", &good).sha1(Some(sha)).size(Some(3)),
            DownloadTask::new("u2", dir.path().join("missing")),
            DownloadTask::new("u3", &good).size(Some(4)),
        ];
        // Same path as the first task: deduplicated, so the size-4 entry is ignored.
        let pending = filter_pending(tasks.clone(), Verify::Size).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].url, "u2");

        std::fs::write(&good, b"abd").unwrap();
        let pending = filter_pending(tasks, Verify::Hash).await.unwrap();
        assert_eq!(
            pending.iter().map(|t| t.url.as_str()).collect::<Vec<_>>(),
            ["u1", "u2"]
        );
    }
}
