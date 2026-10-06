//! A placeholder [`Backend`] that is never connected.
//!
//! It exists so the app and the FFI layer can be wired up in M1.1 before the
//! mock peer lands in M1.2.

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::backend::{
    Backend, BackendError, ConnectionState, Entry, Event, PeerInfo, RemotePath, TransferId,
    TransferRequest,
};

/// A backend with no peer: every remote operation fails with
/// [`BackendError::NotConnected`].
#[derive(Debug)]
pub struct StubBackend {
    events: broadcast::Sender<Event>,
}

impl StubBackend {
    #[must_use]
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(16);
        Self { events }
    }
}

impl Default for StubBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Backend for StubBackend {
    fn peer(&self) -> PeerInfo {
        PeerInfo {
            alias: "peer".to_owned(),
            device_name: "Other Mac".to_owned(),
            model: None,
            device_id: "STUB-STUB-STUB-STUB-STUB".to_owned(),
        }
    }

    fn connection(&self) -> ConnectionState {
        ConnectionState::Disconnected {
            reason: "No backend yet (stub)".to_owned(),
        }
    }

    async fn list_dir(&self, _path: RemotePath) -> Result<Vec<Entry>, BackendError> {
        Err(BackendError::NotConnected)
    }

    async fn start_transfer(&self, _req: TransferRequest) -> Result<TransferId, BackendError> {
        Err(BackendError::NotConnected)
    }

    async fn cancel(&self, _id: TransferId) -> Result<(), BackendError> {
        Err(BackendError::NotFound)
    }

    fn events(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_is_never_connected() {
        let backend = StubBackend::new();
        assert!(matches!(
            backend.connection(),
            ConnectionState::Disconnected { .. }
        ));
        assert_eq!(
            backend.list_dir(RemotePath::root("desktop")).await,
            Err(BackendError::NotConnected)
        );
        let req = TransferRequest::Get {
            remote: vec![RemotePath::root("desktop")],
            dest: std::env::temp_dir(),
        };
        assert_eq!(
            backend.start_transfer(req).await,
            Err(BackendError::NotConnected)
        );
        assert_eq!(
            backend.cancel(TransferId(1)).await,
            Err(BackendError::NotFound)
        );
    }

    #[tokio::test]
    async fn stub_emits_no_events() {
        let backend = StubBackend::new();
        let mut rx = backend.events();
        assert!(matches!(
            rx.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn backend_is_object_safe() {
        let backend: std::sync::Arc<dyn Backend> = std::sync::Arc::new(StubBackend::new());
        assert_eq!(backend.peer().device_name, "Other Mac");
    }
}
