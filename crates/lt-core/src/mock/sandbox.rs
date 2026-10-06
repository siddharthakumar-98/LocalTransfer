//! Where the mock peer is allowed to write: `<sandbox root>/Received/`.
//!
//! Every write goes through a `cap_std::fs::Dir` opened on the sandbox root,
//! so paths cannot escape it, even via `..` or symlinks. Files are written to a
//! create-exclusive partial file and committed with a hard link, which fails
//! instead of replacing an existing file (ROADMAP.md §3, §5).

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use cap_std::ambient_authority;
use cap_std::fs::{Dir, File, OpenOptions};

use super::tree::first_free;
use crate::backend::TransferId;

/// Subfolder of the sandbox root that receives files.
pub(crate) const RECEIVED: &str = "Received";

/// Marker in the names of in-progress files.
pub const PARTIAL_MARKER: &str = ".lt-partial-";

#[derive(Debug)]
pub(crate) struct Sandbox {
    root_path: PathBuf,
    root: Dir,
}

impl Sandbox {
    pub(crate) fn open(root_path: &Path) -> io::Result<Self> {
        // The only write outside a capability handle: creating the root itself.
        std::fs::create_dir_all(root_path)?;
        let root = Dir::open_ambient_dir(root_path, ambient_authority())?;
        root.create_dir_all(RECEIVED)?;
        Ok(Self {
            root_path: root_path.to_owned(),
            root,
        })
    }

    pub(crate) fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub(crate) fn received_path(&self) -> PathBuf {
        self.root_path.join(RECEIVED)
    }

    fn received(&self) -> io::Result<Dir> {
        self.root.create_dir_all(RECEIVED)?;
        self.root.open_dir(RECEIVED)
    }

    /// The first name in `Received/` not already taken by a file or folder.
    pub(crate) fn unique_top_level(&self, name: &str) -> io::Result<String> {
        let received = self.received()?;
        Ok(first_free(name, |n| received.exists(n)))
    }

    /// Creates `Received/<rel>` (and parents).
    pub(crate) fn create_dir_all(&self, rel: &[String]) -> io::Result<()> {
        self.received()?.create_dir_all(join(rel))
    }

    /// Removes `Received/<rel>` if it is an empty folder.
    pub(crate) fn remove_dir_if_empty(&self, rel: &[String]) {
        if let Ok(received) = self.received() {
            let _ = received.remove_dir(join(rel));
        }
    }

    /// Opens `Received/<rel_dir>/.<name>.lt-partial-<id>` for writing; it must not exist.
    pub(crate) fn create_partial(
        &self,
        rel_dir: &[String],
        name: &str,
        id: TransferId,
    ) -> io::Result<Partial> {
        let received = self.received()?;
        let dir = if rel_dir.is_empty() {
            received
        } else {
            received.create_dir_all(join(rel_dir))?;
            received.open_dir(join(rel_dir))?
        };
        let temp_name = format!(".{name}{PARTIAL_MARKER}{}", id.0);
        let file = dir.open_with(&temp_name, OpenOptions::new().write(true).create_new(true))?;
        Ok(Partial {
            dir,
            dir_path: self.received_path().join(join(rel_dir)),
            temp_name,
            final_name: name.to_owned(),
            file,
        })
    }

    /// Empties `Received/`.
    pub(crate) fn reset(&self) -> io::Result<()> {
        match self.root.remove_dir_all(RECEIVED) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        self.root.create_dir(RECEIVED)
    }
}

/// A file being received. Commit or discard it; dropping it leaves the
/// partial file in place (as resume would want).
#[derive(Debug)]
pub(crate) struct Partial {
    dir: Dir,
    dir_path: PathBuf,
    temp_name: String,
    final_name: String,
    file: File,
}

impl Partial {
    pub(crate) fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.file.write_all(buf)
    }

    /// Extends the file to `len` without writing data (a sparse file on APFS).
    pub(crate) fn set_len(&self, len: u64) -> io::Result<()> {
        self.file.set_len(len)
    }

    /// Flushes to disk, then publishes the file under the first free name
    /// (`x`, `x (1)`, …) and removes the partial. Never replaces a file.
    /// Returns the final path and whether the name changed.
    pub(crate) fn commit(self) -> io::Result<(PathBuf, bool)> {
        self.file.sync_all()?;
        let mut n = 0;
        let final_name = loop {
            let candidate = crate::naming::numbered_name(&self.final_name, n);
            match self.dir.hard_link(&self.temp_name, &self.dir, &candidate) {
                Ok(()) => break candidate,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => n += 1,
                Err(e) => return Err(e),
            }
        };
        self.dir.remove_file(&self.temp_name)?;
        Ok((self.dir_path.join(&final_name), n > 0))
    }

    /// Deletes the partial file.
    pub(crate) fn discard(self) -> io::Result<()> {
        self.dir.remove_file(&self.temp_name)
    }
}

fn join(rel: &[String]) -> PathBuf {
    rel.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_never_replaces_and_numbers_clashes() {
        let tmp = tempfile::tempdir().unwrap();
        let sandbox = Sandbox::open(&tmp.path().join("sb")).unwrap();
        let mut names = Vec::new();
        for (i, body) in [b"one", b"two", b"333"].iter().enumerate() {
            let mut p = sandbox
                .create_partial(&[], "x.zip", TransferId(i as u64))
                .unwrap();
            p.write_all(*body).unwrap();
            let (path, renamed) = p.commit().unwrap();
            assert_eq!(renamed, i > 0);
            names.push(path);
        }
        let received = sandbox.received_path();
        assert_eq!(names[0], received.join("x.zip"));
        assert_eq!(names[1], received.join("x (1).zip"));
        assert_eq!(std::fs::read(received.join("x.zip")).unwrap(), b"one");
        assert_eq!(std::fs::read(received.join("x (2).zip")).unwrap(), b"333");
        let leftovers: Vec<_> = std::fs::read_dir(&received)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(PARTIAL_MARKER))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn discard_removes_partial_and_reset_empties() {
        let tmp = tempfile::tempdir().unwrap();
        let sandbox = Sandbox::open(tmp.path()).unwrap();
        let p = sandbox
            .create_partial(&["a".into(), "b".into()], "f", TransferId(9))
            .unwrap();
        p.discard().unwrap();
        assert!(!sandbox.received_path().join("a/b/.f.lt-partial-9").exists());
        sandbox.reset().unwrap();
        assert_eq!(
            std::fs::read_dir(sandbox.received_path()).unwrap().count(),
            0
        );
    }

    #[test]
    fn capability_handle_refuses_escapes() {
        let tmp = tempfile::tempdir().unwrap();
        let sandbox = Sandbox::open(&tmp.path().join("sb")).unwrap();
        let escape = sandbox.create_partial(&["..".into(), "..".into()], "evil", TransferId(1));
        assert!(escape.is_err());
        assert!(!tmp.path().join(".evil.lt-partial-1").exists());
    }
}
