//! LocalTransfer core.
//!
//! The GUI (M1), the CLI (M2) and the real network node (M2) all talk to "the
//! other Mac" through the [`Backend`] trait defined here. See ROADMAP.md §5.
//!
//! After M1 merges, the trait and its types are append-only: methods may be
//! added (with default implementations) and enum variants appended, but nothing
//! is renamed or removed.

#![forbid(unsafe_code)]

pub mod backend;
pub mod mock;
pub mod naming;
pub mod stub;

pub use backend::{
    Backend, BackendError, ConnectionState, Direction, Entry, EntryKind, Event, LinkQuality,
    Outcome, PeerInfo, RemotePath, TransferId, TransferRequest,
};
pub use mock::{MockBackend, MockConfig, MockControl};
pub use stub::StubBackend;

/// The version of `lt-core`, as declared in its Cargo manifest.
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_manifest() {
        assert_eq!(super::version(), env!("CARGO_PKG_VERSION"));
    }
}
