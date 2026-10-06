//! FFI surface of the mock peer's debug controls.

use std::path::PathBuf;
use std::sync::Arc;

use lt_core::MockControl as _;

use crate::types::BackendError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MockProfile {
    /// ~1 GiB/s, no latency. For UI tests and demos.
    Fast,
    /// ~80 MB/s, 3 ms RTT.
    WifiGood,
    /// ~3 MB/s, 120 ms RTT, jitter and stalls.
    Weak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MockLink {
    Good,
    Weak,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DropKind {
    /// Back after 3 s; transfers suspend and resume.
    Blip,
    /// Gone until `set_link`; transfers fail after 10 s.
    Permanent,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MockConfig {
    /// The only folder the mock writes to.
    pub sandbox_root: String,
    pub profile: MockProfile,
    pub seed: u64,
}

/// `~/Library/Application Support/LocalTransfer/MockSandbox`.
#[uniffi::export]
#[must_use]
pub fn default_mock_sandbox_root() -> Option<String> {
    lt_core::mock::default_sandbox_root().map(|p| p.display().to_string())
}

/// Debug controls for the mock peer (Debug menu, `--mock`).
#[derive(uniffi::Object)]
pub struct MockControl {
    inner: Arc<lt_core::MockBackend>,
}

impl MockControl {
    pub(crate) fn new(inner: Arc<lt_core::MockBackend>) -> Arc<Self> {
        Arc::new(Self { inner })
    }
}

#[uniffi::export]
impl MockControl {
    pub fn set_link(&self, link: MockLink) {
        self.inner.set_link(match link {
            MockLink::Good => lt_core::mock::MockLink::Good,
            MockLink::Weak => lt_core::mock::MockLink::Weak,
            MockLink::Down => lt_core::mock::MockLink::Down,
        });
    }

    pub fn set_profile(&self, profile: MockProfile) {
        self.inner.set_profile(profile.into());
    }

    pub fn drop_connection(&self, kind: DropKind) {
        self.inner.drop_connection(match kind {
            DropKind::Blip => lt_core::mock::DropKind::Blip,
            DropKind::Permanent => lt_core::mock::DropKind::Permanent,
        });
    }

    /// Drops the link once the next transfer reaches `percent` of its bytes.
    pub fn drop_at(&self, percent: u8) {
        self.inner.drop_at(percent);
    }

    /// Deletes everything received so far.
    ///
    /// # Errors
    /// [`BackendError::Io`] if the sandbox can't be emptied.
    pub fn reset_sandbox(&self) -> Result<(), BackendError> {
        Ok(self.inner.reset_sandbox()?)
    }

    #[must_use]
    pub fn sandbox_root(&self) -> String {
        self.inner.sandbox_root().display().to_string()
    }

    /// Where received files land (for "Reveal in Finder").
    #[must_use]
    pub fn received_dir(&self) -> String {
        self.inner.received_dir().display().to_string()
    }
}

impl From<MockProfile> for lt_core::mock::Profile {
    fn from(p: MockProfile) -> Self {
        match p {
            MockProfile::Fast => Self::Fast,
            MockProfile::WifiGood => Self::WifiGood,
            MockProfile::Weak => Self::Weak,
        }
    }
}

impl From<MockConfig> for lt_core::MockConfig {
    fn from(c: MockConfig) -> Self {
        let mut config = lt_core::MockConfig::new(PathBuf::from(c.sandbox_root));
        config.profile = c.profile.into();
        config.seed = c.seed;
        config
    }
}
