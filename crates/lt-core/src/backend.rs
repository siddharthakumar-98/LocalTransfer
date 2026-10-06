//! The backend boundary between the UI layers and whatever plays "the other Mac".
//!
//! M1 implements it with a mock peer, M2 with the real QUIC node. The shapes
//! here follow ROADMAP.md §5; `BackendError` mirrors the wire `ErrorCode`s (§6)
//! where they overlap.

use std::path::PathBuf;
use std::time::SystemTime;

use async_trait::async_trait;
use tokio::sync::broadcast;

/// Identifies one transfer for the lifetime of a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TransferId(pub u64);

/// Who the other Mac is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    /// Short name chosen at pairing time, e.g. `air`.
    pub alias: String,
    /// The Mac's own name, e.g. "MacBook Air".
    pub device_name: String,
    /// Hardware model identifier, if known.
    pub model: Option<String>,
    /// Grouped device ID, e.g. `ABCD-EFGH-IJKL-MNOP-QRST`.
    pub device_id: String,
}

/// How good the link to the peer currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkQuality {
    Good,
    Weak,
}

/// Connection status shown by the GUI's indicator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connected { quality: LinkQuality },
    Connecting,
    Disconnected { reason: String },
}

/// A path inside one of the peer's shared roots, as a list of components.
///
/// Paths never travel as slash-delimited strings (ROADMAP.md §3).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemotePath {
    /// Shared-root name, e.g. `desktop`.
    pub root: String,
    pub components: Vec<String>,
}

impl RemotePath {
    /// The top of a shared root.
    #[must_use]
    pub fn root(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            components: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

/// One row of a remote directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: SystemTime,
    /// Uniform Type Identifier, if the backend knows it.
    pub type_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferRequest {
    /// This Mac → peer.
    Send {
        local: Vec<PathBuf>,
        dest: RemotePath,
    },
    /// Peer → this Mac.
    Get {
        remote: Vec<RemotePath>,
        dest: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Send,
    Get,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Cancelled,
    Failed(BackendError),
}

/// Progress and status notifications, delivered through [`Backend::events`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Connection(ConnectionState),
    TransferStarted {
        id: TransferId,
        direction: Direction,
        items: u32,
        total_bytes: u64,
    },
    /// Emitted at most 10 times per second per transfer.
    TransferProgress {
        id: TransferId,
        bytes_done: u64,
        total_bytes: u64,
        bytes_per_sec: u64,
        current_file: String,
    },
    /// The connection was lost; the backend will try to resume.
    TransferSuspended {
        id: TransferId,
        reason: BackendError,
    },
    TransferResumed {
        id: TransferId,
    },
    FileCommitted {
        id: TransferId,
        final_path: String,
        renamed: bool,
    },
    TransferFinished {
        id: TransferId,
        outcome: Outcome,
    },
    /// Hint that the peer pane showing `path` should refresh.
    RemoteChanged {
        path: RemotePath,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
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
    #[error("file system error: {0}")]
    Io(String),
    #[error("internal error: {0}")]
    Internal(String),
}

/// Everything the UI layers need from "the other Mac".
#[async_trait]
pub trait Backend: Send + Sync + 'static {
    /// Who the peer is.
    fn peer(&self) -> PeerInfo;

    /// The current connection state; changes are also sent as [`Event::Connection`].
    fn connection(&self) -> ConnectionState;

    /// Lists one directory on the peer.
    async fn list_dir(&self, path: RemotePath) -> Result<Vec<Entry>, BackendError>;

    /// Starts a transfer in either direction. Progress arrives as [`Event`]s.
    async fn start_transfer(&self, req: TransferRequest) -> Result<TransferId, BackendError>;

    /// Cancels a running transfer. It finishes with [`Outcome::Cancelled`].
    async fn cancel(&self, id: TransferId) -> Result<(), BackendError>;

    /// Subscribes to progress and status events.
    fn events(&self) -> broadcast::Receiver<Event>;
}
