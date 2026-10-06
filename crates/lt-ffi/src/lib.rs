//! UniFFI bindings over `lt-core`, consumed by the SwiftUI app.
//!
//! This is the one crate without `#![forbid(unsafe_code)]`: UniFFI's macros
//! expand to `extern "C"` scaffolding that uses `unsafe` internally. There is
//! no hand-written `unsafe` here, and none may be added without a
//! `// SAFETY:` comment and review.
//!
//! The types below mirror `lt_core::backend` one to one, so `lt-core` stays
//! free of UniFFI attributes. The Swift bindings generated from this crate are
//! a build output (see `scripts/build-rust.sh`) and are never committed.

#![deny(unsafe_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use lt_core::backend as core;

uniffi::setup_scaffolding!();

/// The version of `lt-core`.
#[uniffi::export]
#[must_use]
pub fn lt_core_version() -> String {
    lt_core::version()
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PeerInfo {
    pub alias: String,
    pub device_name: String,
    pub model: Option<String>,
    pub device_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LinkQuality {
    Good,
    Weak,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionState {
    Connected { quality: LinkQuality },
    Connecting,
    Disconnected { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RemotePath {
    pub root: String,
    pub components: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: SystemTime,
    pub type_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TransferRequest {
    Send {
        local: Vec<String>,
        dest: RemotePath,
    },
    Get {
        remote: Vec<RemotePath>,
        dest: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum BackendError {
    #[error("not connected to the other Mac")]
    NotConnected,
    #[error("the connection to the other Mac was lost")]
    ConnectionLost,
    #[error("no such file or folder")]
    NotFound,
    #[error("permission denied")]
    PermissionDenied,
    #[error("that path is outside the shared folders")]
    OutsideRoot,
    #[error("invalid path")]
    InvalidPath,
    #[error("a file with that name already exists")]
    Conflict,
    #[error("the received data failed its integrity check")]
    HashMismatch,
    #[error("file system error: {message}")]
    Io { message: String },
    #[error("internal error: {message}")]
    Internal { message: String },
}

/// A handle to a backend, as seen from Swift.
#[derive(uniffi::Object)]
pub struct CoreBackend {
    inner: Arc<dyn core::Backend>,
}

#[uniffi::export]
impl CoreBackend {
    /// A backend with no peer. Replaced by the mock peer in M1.2.
    #[uniffi::constructor]
    #[must_use]
    pub fn stub() -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(lt_core::StubBackend::new()),
        })
    }

    #[must_use]
    pub fn peer(&self) -> PeerInfo {
        self.inner.peer().into()
    }

    #[must_use]
    pub fn connection(&self) -> ConnectionState {
        self.inner.connection().into()
    }

    /// Lists one directory on the peer.
    ///
    /// # Errors
    /// Any [`BackendError`] the backend reports.
    pub async fn list_dir(&self, path: RemotePath) -> Result<Vec<Entry>, BackendError> {
        let entries = self.inner.list_dir(path.into()).await?;
        Ok(entries.into_iter().map(Into::into).collect())
    }

    /// Starts a transfer and returns its ID.
    ///
    /// # Errors
    /// Any [`BackendError`] the backend reports.
    pub async fn start_transfer(&self, req: TransferRequest) -> Result<u64, BackendError> {
        Ok(self.inner.start_transfer(req.into()).await?.0)
    }

    /// Cancels a running transfer.
    ///
    /// # Errors
    /// Any [`BackendError`] the backend reports.
    pub async fn cancel(&self, id: u64) -> Result<(), BackendError> {
        Ok(self.inner.cancel(core::TransferId(id)).await?)
    }
}

// ---- conversions between lt-core and the FFI mirror types ----

impl From<core::PeerInfo> for PeerInfo {
    fn from(p: core::PeerInfo) -> Self {
        Self {
            alias: p.alias,
            device_name: p.device_name,
            model: p.model,
            device_id: p.device_id,
        }
    }
}

impl From<core::LinkQuality> for LinkQuality {
    fn from(q: core::LinkQuality) -> Self {
        match q {
            core::LinkQuality::Good => Self::Good,
            core::LinkQuality::Weak => Self::Weak,
        }
    }
}

impl From<core::ConnectionState> for ConnectionState {
    fn from(s: core::ConnectionState) -> Self {
        match s {
            core::ConnectionState::Connected { quality } => Self::Connected {
                quality: quality.into(),
            },
            core::ConnectionState::Connecting => Self::Connecting,
            core::ConnectionState::Disconnected { reason } => Self::Disconnected { reason },
        }
    }
}

impl From<RemotePath> for core::RemotePath {
    fn from(p: RemotePath) -> Self {
        Self {
            root: p.root,
            components: p.components,
        }
    }
}

impl From<core::EntryKind> for EntryKind {
    fn from(k: core::EntryKind) -> Self {
        match k {
            core::EntryKind::File => Self::File,
            core::EntryKind::Dir => Self::Dir,
            core::EntryKind::Symlink => Self::Symlink,
        }
    }
}

impl From<core::Entry> for Entry {
    fn from(e: core::Entry) -> Self {
        Self {
            name: e.name,
            kind: e.kind.into(),
            size: e.size,
            modified: e.modified,
            type_hint: e.type_hint,
        }
    }
}

impl From<TransferRequest> for core::TransferRequest {
    fn from(r: TransferRequest) -> Self {
        match r {
            TransferRequest::Send { local, dest } => Self::Send {
                local: local.into_iter().map(PathBuf::from).collect(),
                dest: dest.into(),
            },
            TransferRequest::Get { remote, dest } => Self::Get {
                remote: remote.into_iter().map(Into::into).collect(),
                dest: PathBuf::from(dest),
            },
        }
    }
}

impl From<core::BackendError> for BackendError {
    fn from(e: core::BackendError) -> Self {
        match e {
            core::BackendError::NotConnected => Self::NotConnected,
            core::BackendError::ConnectionLost => Self::ConnectionLost,
            core::BackendError::NotFound => Self::NotFound,
            core::BackendError::PermissionDenied => Self::PermissionDenied,
            core::BackendError::OutsideRoot => Self::OutsideRoot,
            core::BackendError::InvalidPath => Self::InvalidPath,
            core::BackendError::Conflict => Self::Conflict,
            core::BackendError::HashMismatch => Self::HashMismatch,
            core::BackendError::Io(message) => Self::Io { message },
            core::BackendError::Internal(message) => Self::Internal { message },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comes_from_lt_core() {
        assert_eq!(lt_core_version(), lt_core::version());
    }

    #[test]
    fn stub_backend_round_trips_through_mirror_types() {
        let backend = CoreBackend::stub();
        assert_eq!(backend.peer().device_name, "Other Mac");
        assert!(matches!(
            backend.connection(),
            ConnectionState::Disconnected { .. }
        ));
    }

    #[test]
    fn errors_map_one_to_one() {
        assert_eq!(
            BackendError::from(core::BackendError::Io("disk full".into())),
            BackendError::Io {
                message: "disk full".into()
            }
        );
        assert_eq!(
            BackendError::from(core::BackendError::NotConnected),
            BackendError::NotConnected
        );
    }
}
