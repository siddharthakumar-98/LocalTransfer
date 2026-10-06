//! FFI mirrors of `lt_core::backend`, with conversions.

use std::path::PathBuf;
use std::time::SystemTime;

use lt_core::backend as core;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Direction {
    Send,
    Get,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Outcome {
    Completed,
    Cancelled,
    Failed { error: BackendError },
}

/// Progress and status notifications, delivered to an `EventListener`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Event {
    Connection {
        state: ConnectionState,
    },
    TransferStarted {
        id: u64,
        direction: Direction,
        items: u32,
        total_bytes: u64,
    },
    TransferProgress {
        id: u64,
        bytes_done: u64,
        total_bytes: u64,
        bytes_per_sec: u64,
        current_file: String,
    },
    TransferSuspended {
        id: u64,
        reason: BackendError,
    },
    TransferResumed {
        id: u64,
    },
    FileCommitted {
        id: u64,
        final_path: String,
        renamed: bool,
    },
    TransferFinished {
        id: u64,
        outcome: Outcome,
    },
    RemoteChanged {
        path: RemotePath,
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

// ---- conversions ----

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

impl From<core::RemotePath> for RemotePath {
    fn from(p: core::RemotePath) -> Self {
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

impl From<core::Direction> for Direction {
    fn from(d: core::Direction) -> Self {
        match d {
            core::Direction::Send => Self::Send,
            core::Direction::Get => Self::Get,
        }
    }
}

impl From<core::Outcome> for Outcome {
    fn from(o: core::Outcome) -> Self {
        match o {
            core::Outcome::Completed => Self::Completed,
            core::Outcome::Cancelled => Self::Cancelled,
            core::Outcome::Failed(e) => Self::Failed { error: e.into() },
        }
    }
}

impl From<core::Event> for Event {
    fn from(e: core::Event) -> Self {
        match e {
            core::Event::Connection(state) => Self::Connection {
                state: state.into(),
            },
            core::Event::TransferStarted {
                id,
                direction,
                items,
                total_bytes,
            } => Self::TransferStarted {
                id: id.0,
                direction: direction.into(),
                items,
                total_bytes,
            },
            core::Event::TransferProgress {
                id,
                bytes_done,
                total_bytes,
                bytes_per_sec,
                current_file,
            } => Self::TransferProgress {
                id: id.0,
                bytes_done,
                total_bytes,
                bytes_per_sec,
                current_file,
            },
            core::Event::TransferSuspended { id, reason } => Self::TransferSuspended {
                id: id.0,
                reason: reason.into(),
            },
            core::Event::TransferResumed { id } => Self::TransferResumed { id: id.0 },
            core::Event::FileCommitted {
                id,
                final_path,
                renamed,
            } => Self::FileCommitted {
                id: id.0,
                final_path,
                renamed,
            },
            core::Event::TransferFinished { id, outcome } => Self::TransferFinished {
                id: id.0,
                outcome: outcome.into(),
            },
            core::Event::RemoteChanged { path } => Self::RemoteChanged { path: path.into() },
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

    #[test]
    fn events_keep_their_payload() {
        let e = Event::from(core::Event::TransferFinished {
            id: core::TransferId(7),
            outcome: core::Outcome::Failed(core::BackendError::ConnectionLost),
        });
        assert_eq!(
            e,
            Event::TransferFinished {
                id: 7,
                outcome: Outcome::Failed {
                    error: BackendError::ConnectionLost
                }
            }
        );
    }
}
