//! UniFFI bindings over `lt-core`, consumed by the SwiftUI app.
//!
//! This is the one crate without `#![forbid(unsafe_code)]`: UniFFI's macros
//! expand to `extern "C"` scaffolding that uses `unsafe` internally. There is
//! no hand-written `unsafe` here, and none may be added without a
//! `// SAFETY:` comment and review.
//!
//! The types in [`types`] mirror `lt_core::backend` one to one, so `lt-core`
//! stays free of UniFFI attributes. The Swift bindings generated from this
//! crate are a build output (see `scripts/build-rust.sh`) and are never
//! committed.
//!
//! This crate owns the tokio runtime that backends run on. Async methods called
//! from Swift hop onto it, and events reach Swift through [`EventListener`],
//! called from a runtime thread.

#![deny(unsafe_code)]

mod backend;
mod mock;
mod types;

use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

pub use backend::{CoreBackend, EventListener};
pub use mock::{DropKind, MockConfig, MockControl, MockLink, MockProfile};
pub use types::*;

uniffi::setup_scaffolding!();

/// The version of `lt-core`.
#[uniffi::export]
#[must_use]
pub fn lt_core_version() -> String {
    lt_core::version()
}

/// The runtime every backend task runs on, created on first use.
pub(crate) fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("lt-core")
            .enable_time()
            .build()
            .expect("the tokio runtime starts")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comes_from_lt_core() {
        assert_eq!(lt_core_version(), lt_core::version());
    }
}
