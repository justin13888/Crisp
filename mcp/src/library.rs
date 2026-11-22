//! The local recipe library: one `<doc_id>.crisp` file per document, holding
//! its latest sealed (or endorsed) revision.
//!
//! The trait is async, and [`FsLibrary`] runs each operation's blocking
//! `std::fs` calls on Tokio's blocking pool, so a slow disk never stalls an
//! async worker thread or the tool calls sharing it.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes the temp files of concurrent writes in one process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

use async_trait::async_trait;
use tracing::{debug, info};

/// A library operation that failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct LibraryError(pub String);

/// Storage for `.crisp` files, keyed by document id.
///
/// `list`, `read`, and `write` may touch the disk, so they are async and an
/// implementation must not block the calling worker thread.
#[async_trait]
pub trait Library: Send + Sync {
    /// Every stored document id, sorted.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryError`] if the store cannot be listed.
    async fn list(&self) -> Result<Vec<String>, LibraryError>;

    /// The stored file for `doc_id`, or `None`.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryError`] for a malformed id or an unreadable store.
    async fn read(&self, doc_id: &str) -> Result<Option<Vec<u8>>, LibraryError>;

    /// Stores `bytes` as the file for `doc_id`, replacing any earlier one.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryError`] for a malformed id or a failed write.
    async fn write(&self, doc_id: &str, bytes: &[u8]) -> Result<(), LibraryError>;

    /// A human-readable location, for status output.
    fn location(&self) -> String;
}

/// Accepts only a well-formed recipe or book id, so an id can never name a
/// path outside the library.
fn checked(doc_id: &str) -> Result<&str, LibraryError> {
    let well_formed = doc_id.len() == 63
        && (doc_id.starts_with("rcp1") || doc_id.starts_with("rbk1"))
        && doc_id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    if well_formed {
        Ok(doc_id)
    } else {
        Err(LibraryError(format!(
            "{doc_id:?} is not a recipe id (expected 63 characters starting with rcp1)"
        )))
    }
}

/// A library in a directory on disk.
#[derive(Debug, Clone)]
pub struct FsLibrary {
    dir: PathBuf,
}

impl FsLibrary {
    /// A library rooted at `dir`, created on first write.
    #[must_use]
    pub const fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, doc_id: &str) -> Result<PathBuf, LibraryError> {
        Ok(self.dir.join(format!("{}.crisp", checked(doc_id)?)))
    }
}

/// Runs `io` on Tokio's blocking pool, so its `std::fs` calls never occupy
/// an async worker thread. A task that panicked or was cancelled becomes a
/// [`LibraryError`] naming `op`.
async fn off_runtime<T, F>(op: &'static str, io: F) -> Result<T, LibraryError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, LibraryError> + Send + 'static,
{
    let started = std::time::Instant::now();
    let result = tokio::task::spawn_blocking(io)
        .await
        .map_err(|e| LibraryError(format!("the library {op} did not finish: {e}")))?;
    debug!(
        op,
        ok = result.is_ok(),
        elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        "library I/O finished"
    );
    result
}

/// Every well-formed document id with a `.crisp` file in `dir`, sorted; none
/// when `dir` does not exist yet. Blocks.
fn list_dir(dir: &Path) -> Result<Vec<String>, LibraryError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(LibraryError(format!(
                "could not list {}: {e}",
                dir.display()
            )));
        }
    };
    let mut ids: Vec<String> = entries
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            let id = name.strip_suffix(".crisp")?;
            checked(id).ok().map(str::to_owned)
        })
        .collect();
    ids.sort();
    debug!(dir = %dir.display(), count = ids.len(), "library listed");
    Ok(ids)
}

/// The contents of `path`, or `None` when it does not exist. Blocks.
fn read_file(path: &Path) -> Result<Option<Vec<u8>>, LibraryError> {
    match std::fs::read(path) {
        Ok(bytes) => {
            debug!(path = %path.display(), bytes = bytes.len(), "library file read");
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!(path = %path.display(), "library file absent");
            Ok(None)
        }
        Err(e) => Err(LibraryError(format!(
            "could not read {}: {e}",
            path.display()
        ))),
    }
}

/// Writes `bytes` to `path` through a temporary file in `dir` and renames it
/// into place, so a crash never leaves a torn `.crisp` file. Blocks.
fn write_file(dir: &Path, path: &Path, doc_id: &str, bytes: &[u8]) -> Result<(), LibraryError> {
    let fail = |what: &str, e: std::io::Error| {
        LibraryError(format!("could not {what} {}: {e}", path.display()))
    };
    std::fs::create_dir_all(dir).map_err(|e| fail("create the library for", e))?;
    // A temp name unique to this write, so concurrent writes never share one.
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = dir.join(format!(".{doc_id}.{}.{unique}.tmp", std::process::id()));
    debug!(doc_id, temp = %temp.display(), "writing library temp file");
    let written = (|| {
        let mut file = std::fs::File::create(&temp).map_err(|e| fail("write", e))?;
        file.write_all(bytes).map_err(|e| fail("write", e))?;
        file.sync_all().map_err(|e| fail("flush", e))?;
        drop(file);
        std::fs::rename(&temp, path).map_err(|e| fail("replace", e))
    })();
    if written.is_err() {
        // Best effort: a leftover temp file is harmless but untidy.
        let _ = std::fs::remove_file(&temp);
    }
    written?;
    info!(doc_id, bytes = bytes.len(), path = %path.display(), "recipe stored");
    Ok(())
}

#[async_trait]
impl Library for FsLibrary {
    async fn list(&self) -> Result<Vec<String>, LibraryError> {
        let dir = self.dir.clone();
        off_runtime("listing", move || list_dir(&dir)).await
    }

    async fn read(&self, doc_id: &str) -> Result<Option<Vec<u8>>, LibraryError> {
        let path = self.path(doc_id)?;
        off_runtime("read", move || read_file(&path)).await
    }

    /// Writes through a temporary file in the same directory and renames it
    /// into place, so a crash never leaves a torn `.crisp` file.
    async fn write(&self, doc_id: &str, bytes: &[u8]) -> Result<(), LibraryError> {
        let path = self.path(doc_id)?;
        let (dir, doc_id, bytes) = (self.dir.clone(), doc_id.to_owned(), bytes.to_vec());
        off_runtime("write", move || write_file(&dir, &path, &doc_id, &bytes)).await
    }

    fn location(&self) -> String {
        self.dir.display().to_string()
    }
}

/// An in-memory library for tests.
#[derive(Debug, Default)]
pub struct MemoryLibrary {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MemoryLibrary {
    fn files(&self) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, Vec<u8>>>, LibraryError> {
        self.files
            .lock()
            .map_err(|_| LibraryError("the library lock was poisoned".into()))
    }
}

#[async_trait]
impl Library for MemoryLibrary {
    async fn list(&self) -> Result<Vec<String>, LibraryError> {
        Ok(self.files()?.keys().cloned().collect())
    }

    async fn read(&self, doc_id: &str) -> Result<Option<Vec<u8>>, LibraryError> {
        Ok(self.files()?.get(checked(doc_id)?).cloned())
    }

    async fn write(&self, doc_id: &str, bytes: &[u8]) -> Result<(), LibraryError> {
        self.files()?
            .insert(checked(doc_id)?.to_owned(), bytes.to_vec());
        Ok(())
    }

    fn location(&self) -> String {
        "memory".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "rcp1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";
    const OTHER: &str = "rcp1zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";

    async fn exercise(library: &dyn Library) {
        assert_eq!(library.list().await.expect("list"), Vec::<String>::new());
        assert_eq!(library.read(ID).await.expect("read"), None);
        library.write(OTHER, b"two").await.expect("write");
        library.write(ID, b"one").await.expect("write");
        library.write(ID, b"one, revised").await.expect("overwrite");
        assert_eq!(
            library.list().await.expect("list"),
            vec![ID.to_owned(), OTHER.to_owned()]
        );
        assert_eq!(
            library.read(ID).await.expect("read"),
            Some(b"one, revised".to_vec())
        );
        for bad in [
            "../etc/passwd",
            "rcp1short",
            &ID.replace('q', "Q"),
            &ID.replacen("rcp", "usr", 1),
        ] {
            assert!(library.read(bad).await.is_err(), "{bad} must be refused");
            assert!(
                library.write(bad, b"x").await.is_err(),
                "{bad} must be refused"
            );
        }
    }

    #[tokio::test]
    async fn the_memory_library_honours_the_contract() {
        exercise(&MemoryLibrary::default()).await;
        assert_eq!(MemoryLibrary::default().location(), "memory");
    }

    #[tokio::test]
    async fn the_file_library_honours_the_contract_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = FsLibrary::new(dir.path().join("library"));
        exercise(&library).await;
        std::fs::write(dir.path().join("library/notes.txt"), b"ignored").expect("write");
        assert_eq!(library.list().await.expect("list").len(), 2);
        let names: Vec<String> = std::fs::read_dir(dir.path().join("library"))
            .expect("dir")
            .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
            .collect();
        assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
        assert!(library.location().ends_with("library"));
    }

    #[tokio::test]
    async fn io_errors_other_than_absence_are_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let not_a_dir = dir.path().join("file");
        std::fs::write(&not_a_dir, b"x").expect("write");
        assert!(FsLibrary::new(not_a_dir).list().await.is_err());
        let library = FsLibrary::new(dir.path().to_path_buf());
        std::fs::create_dir(dir.path().join(format!("{ID}.crisp"))).expect("a directory");
        assert!(library.read(ID).await.is_err());
        assert_eq!(library.read(OTHER).await.expect("absent"), None);
    }

    #[tokio::test]
    async fn a_book_id_is_accepted() {
        let book = ID.replacen("rcp", "rbk", 1);
        let library = MemoryLibrary::default();
        library.write(&book, b"book").await.expect("write");
        assert_eq!(
            library.read(&book).await.expect("read"),
            Some(b"book".to_vec())
        );
    }

    /// On a current-thread runtime, blocking I/O run inline would hold the
    /// only thread, so the sender below could never run and the receive
    /// would time out. Off the runtime, the second task makes progress.
    #[tokio::test(flavor = "current_thread")]
    async fn slow_io_leaves_the_runtime_free_for_other_tasks() {
        let (unblock, blocked) = std::sync::mpsc::channel::<u8>();
        let slow = tokio::spawn(off_runtime("read", move || {
            blocked
                .recv_timeout(std::time::Duration::from_secs(10))
                .map_err(|e| LibraryError(format!("the runtime was blocked: {e}")))
        }));
        let other = tokio::spawn(async move {
            tokio::task::yield_now().await;
            unblock.send(7).map_err(|e| e.to_string())
        });
        other.await.expect("other task").expect("sent");
        assert_eq!(slow.await.expect("slow task"), Ok(7));
    }

    #[tokio::test]
    async fn a_failed_blocking_task_is_a_library_error() {
        let failed = off_runtime::<(), _>("write", || panic!("the disk caught fire")).await;
        let error = failed.expect_err("a panicking task must not succeed");
        assert!(
            error.0.starts_with("the library write did not finish"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn concurrent_file_writes_each_land_whole() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = std::sync::Arc::new(FsLibrary::new(dir.path().to_path_buf()));
        let writes: Vec<_> = (0..8u8)
            .map(|n| {
                let library = std::sync::Arc::clone(&library);
                tokio::spawn(async move { library.write(ID, &[n; 4096]).await })
            })
            .collect();
        for write in writes {
            write.await.expect("joined").expect("written");
        }
        let stored = library.read(ID).await.expect("read").expect("stored");
        assert_eq!(stored.len(), 4096);
        assert!(stored.iter().all(|b| *b == stored[0]), "a torn file");
        assert_eq!(library.list().await.expect("list"), vec![ID.to_owned()]);
    }
}
