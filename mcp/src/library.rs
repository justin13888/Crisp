//! The local recipe library: one `<doc_id>.crisp` file per document, holding
//! its latest sealed (or endorsed) revision.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes the temp files of concurrent writes in one process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

use tracing::{debug, info};

/// A library operation that failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct LibraryError(pub String);

/// Storage for `.crisp` files, keyed by document id.
pub trait Library: Send + Sync {
    /// Every stored document id, sorted.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryError`] if the store cannot be listed.
    fn list(&self) -> Result<Vec<String>, LibraryError>;

    /// The stored file for `doc_id`, or `None`.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryError`] for a malformed id or an unreadable store.
    fn read(&self, doc_id: &str) -> Result<Option<Vec<u8>>, LibraryError>;

    /// Stores `bytes` as the file for `doc_id`, replacing any earlier one.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryError`] for a malformed id or a failed write.
    fn write(&self, doc_id: &str, bytes: &[u8]) -> Result<(), LibraryError>;

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

impl Library for FsLibrary {
    fn list(&self) -> Result<Vec<String>, LibraryError> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(LibraryError(format!(
                    "could not list {}: {e}",
                    self.dir.display()
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
        debug!(count = ids.len(), "library listed");
        Ok(ids)
    }

    fn read(&self, doc_id: &str) -> Result<Option<Vec<u8>>, LibraryError> {
        let path = self.path(doc_id)?;
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(LibraryError(format!(
                "could not read {}: {e}",
                path.display()
            ))),
        }
    }

    /// Writes through a temporary file in the same directory and renames it
    /// into place, so a crash never leaves a torn `.crisp` file.
    fn write(&self, doc_id: &str, bytes: &[u8]) -> Result<(), LibraryError> {
        let path = self.path(doc_id)?;
        let fail = |what: &str, e: std::io::Error| {
            LibraryError(format!("could not {what} {}: {e}", path.display()))
        };
        std::fs::create_dir_all(&self.dir).map_err(|e| fail("create the library for", e))?;
        // A temp name unique to this write, so concurrent writes never share one.
        let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp = self
            .dir
            .join(format!(".{doc_id}.{}.{unique}.tmp", std::process::id()));
        let written = (|| {
            let mut file = std::fs::File::create(&temp).map_err(|e| fail("write", e))?;
            file.write_all(bytes).map_err(|e| fail("write", e))?;
            file.sync_all().map_err(|e| fail("flush", e))?;
            drop(file);
            std::fs::rename(&temp, &path).map_err(|e| fail("replace", e))
        })();
        if written.is_err() {
            // Best effort: a leftover temp file is harmless but untidy.
            let _ = std::fs::remove_file(&temp);
        }
        written?;
        info!(doc_id, bytes = bytes.len(), path = %path.display(), "recipe stored");
        Ok(())
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

impl Library for MemoryLibrary {
    fn list(&self) -> Result<Vec<String>, LibraryError> {
        Ok(self.files()?.keys().cloned().collect())
    }

    fn read(&self, doc_id: &str) -> Result<Option<Vec<u8>>, LibraryError> {
        Ok(self.files()?.get(checked(doc_id)?).cloned())
    }

    fn write(&self, doc_id: &str, bytes: &[u8]) -> Result<(), LibraryError> {
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

    fn exercise(library: &dyn Library) {
        assert_eq!(library.list().expect("list"), Vec::<String>::new());
        assert_eq!(library.read(ID).expect("read"), None);
        library.write(OTHER, b"two").expect("write");
        library.write(ID, b"one").expect("write");
        library.write(ID, b"one, revised").expect("overwrite");
        assert_eq!(
            library.list().expect("list"),
            vec![ID.to_owned(), OTHER.to_owned()]
        );
        assert_eq!(
            library.read(ID).expect("read"),
            Some(b"one, revised".to_vec())
        );
        for bad in [
            "../etc/passwd",
            "rcp1short",
            &ID.replace('q', "Q"),
            &ID.replacen("rcp", "usr", 1),
        ] {
            assert!(library.read(bad).is_err(), "{bad} must be refused");
            assert!(library.write(bad, b"x").is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn the_memory_library_honours_the_contract() {
        exercise(&MemoryLibrary::default());
        assert_eq!(MemoryLibrary::default().location(), "memory");
    }

    #[test]
    fn the_file_library_honours_the_contract_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = FsLibrary::new(dir.path().join("library"));
        exercise(&library);
        std::fs::write(dir.path().join("library/notes.txt"), b"ignored").expect("write");
        assert_eq!(library.list().expect("list").len(), 2);
        let names: Vec<String> = std::fs::read_dir(dir.path().join("library"))
            .expect("dir")
            .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
            .collect();
        assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
        assert!(library.location().ends_with("library"));
    }

    #[test]
    fn io_errors_other_than_absence_are_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let not_a_dir = dir.path().join("file");
        std::fs::write(&not_a_dir, b"x").expect("write");
        assert!(FsLibrary::new(not_a_dir).list().is_err());
        let library = FsLibrary::new(dir.path().to_path_buf());
        std::fs::create_dir(dir.path().join(format!("{ID}.crisp"))).expect("a directory");
        assert!(library.read(ID).is_err());
        assert_eq!(library.read(OTHER).expect("absent"), None);
    }

    #[test]
    fn a_book_id_is_accepted() {
        let book = ID.replacen("rcp", "rbk", 1);
        let library = MemoryLibrary::default();
        library.write(&book, b"book").expect("write");
        assert_eq!(library.read(&book).expect("read"), Some(b"book".to_vec()));
    }
}
