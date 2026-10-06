//! The backend handle Swift holds, and how events reach Swift.

use std::future::Future;
use std::sync::{Arc, Mutex};

use lt_core::backend as core;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use crate::mock::{MockConfig, MockControl};
use crate::runtime;
use crate::types::{
    BackendError, ConnectionState, Entry, Event, PeerInfo, RemotePath, TransferRequest,
};

/// Receives progress and status events. Swift implements this; it is called
/// on a background thread, so implementations must hop to the main actor
/// themselves before touching UI state.
#[uniffi::export(with_foreign)]
pub trait EventListener: Send + Sync {
    fn on_event(&self, event: Event);
}

/// A handle to a backend (the stub, the mock peer, or from M2 the real node).
#[derive(uniffi::Object)]
pub struct CoreBackend {
    inner: Arc<dyn core::Backend>,
    mock: Option<Arc<lt_core::MockBackend>>,
    listener: Mutex<Option<JoinHandle<()>>>,
}

/// Runs `fut` on the shared runtime and waits for it from any executor.
async fn on_runtime<T, F>(fut: F) -> Result<T, BackendError>
where
    T: Send + 'static,
    F: Future<Output = Result<T, core::BackendError>> + Send + 'static,
{
    runtime()
        .spawn(fut)
        .await
        .map_err(|e| BackendError::Internal {
            message: e.to_string(),
        })?
        .map_err(Into::into)
}

impl CoreBackend {
    fn new(inner: Arc<dyn core::Backend>, mock: Option<Arc<lt_core::MockBackend>>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            mock,
            listener: Mutex::new(None),
        })
    }

    fn stop_listener(&self) {
        let mut slot = self
            .listener
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(task) = slot.take() {
            task.abort();
        }
    }
}

impl Drop for CoreBackend {
    fn drop(&mut self) {
        self.stop_listener();
    }
}

#[uniffi::export]
impl CoreBackend {
    /// A backend with no peer: everything fails with `NotConnected`.
    #[uniffi::constructor]
    #[must_use]
    pub fn stub() -> Arc<Self> {
        Self::new(Arc::new(lt_core::StubBackend::new()), None)
    }

    /// The mock peer (ROADMAP.md §5). It only ever writes inside
    /// `config.sandbox_root`.
    ///
    /// # Errors
    /// [`BackendError::Io`] if the sandbox folder can't be created.
    #[uniffi::constructor]
    pub fn mock(config: MockConfig) -> Result<Arc<Self>, BackendError> {
        let mock = Arc::new(lt_core::MockBackend::with_handle(
            config.into(),
            runtime().handle().clone(),
        )?);
        Ok(Self::new(mock.clone(), Some(mock)))
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
        let inner = Arc::clone(&self.inner);
        let entries = on_runtime(async move { inner.list_dir(path.into()).await }).await?;
        Ok(entries.into_iter().map(Into::into).collect())
    }

    /// Starts a transfer and returns its ID. Progress arrives as events.
    ///
    /// # Errors
    /// Any [`BackendError`] the backend reports.
    pub async fn start_transfer(&self, req: TransferRequest) -> Result<u64, BackendError> {
        let inner = Arc::clone(&self.inner);
        let id = on_runtime(async move { inner.start_transfer(req.into()).await }).await?;
        Ok(id.0)
    }

    /// Cancels a running transfer.
    ///
    /// # Errors
    /// Any [`BackendError`] the backend reports.
    pub async fn cancel(&self, id: u64) -> Result<(), BackendError> {
        let inner = Arc::clone(&self.inner);
        on_runtime(async move { inner.cancel(core::TransferId(id)).await }).await
    }

    /// Sends every future event to `listener`, replacing any previous one.
    /// `None` stops delivery.
    pub fn set_event_listener(&self, listener: Option<Arc<dyn EventListener>>) {
        self.stop_listener();
        let Some(listener) = listener else { return };
        let mut events = self.inner.events();
        let task = runtime().spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => listener.on_event(event.into()),
                    // A slow listener missed some progress; later events catch it up.
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
        *self
            .listener
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(task);
    }

    /// Debug controls, if this is the mock peer.
    #[must_use]
    pub fn mock_control(&self) -> Option<Arc<MockControl>> {
        self.mock.as_ref().map(|m| MockControl::new(Arc::clone(m)))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::mock::MockProfile;
    use crate::types::Outcome;

    #[derive(Default)]
    struct Collect(Mutex<Vec<Event>>);

    impl EventListener for Collect {
        fn on_event(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn mock(tmp: &tempfile::TempDir) -> Arc<CoreBackend> {
        CoreBackend::mock(MockConfig {
            sandbox_root: tmp.path().join("sb").display().to_string(),
            profile: MockProfile::Fast,
            seed: 1,
        })
        .unwrap()
    }

    #[test]
    fn stub_backend_is_disconnected() {
        let backend = CoreBackend::stub();
        assert_eq!(backend.peer().device_name, "Other Mac");
        assert!(matches!(
            backend.connection(),
            ConnectionState::Disconnected { .. }
        ));
        let err = runtime().block_on(backend.list_dir(RemotePath {
            root: "desktop".into(),
            components: vec![],
        }));
        assert_eq!(err, Err(BackendError::NotConnected));
    }

    #[test]
    fn mock_lists_and_transfers_with_events() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = mock(&tmp);
        assert_eq!(backend.peer().device_name, "Mock Mac");
        let root = RemotePath {
            root: "desktop".into(),
            components: vec![],
        };
        let entries = runtime().block_on(backend.list_dir(root.clone())).unwrap();
        assert!(entries.iter().any(|e| e.name == "Projects"));

        let events = Arc::new(Collect::default());
        backend.set_event_listener(Some(events.clone()));
        let id = runtime()
            .block_on(backend.start_transfer(TransferRequest::Get {
                remote: vec![RemotePath {
                    root: "desktop".into(),
                    components: vec!["todo.txt".into()],
                }],
                dest: "/ignored".into(),
            }))
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        let finished = loop {
            let found = events.0.lock().unwrap().iter().find_map(|e| match e {
                Event::TransferFinished { id: fid, outcome } if *fid == id => Some(outcome.clone()),
                _ => None,
            });
            if let Some(outcome) = found {
                break outcome;
            }
            assert!(Instant::now() < deadline, "transfer did not finish");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(finished, Outcome::Completed);
        let control = backend.mock_control().expect("mock has controls");
        assert!(
            std::path::Path::new(&control.received_dir())
                .join("todo.txt")
                .is_file()
        );
        assert!(CoreBackend::stub().mock_control().is_none());
    }
}
