//! Helpers shared by the integration tests.

#![allow(dead_code)] // Each test binary uses a different subset.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lt_core::mock::{MockConfig, PARTIAL_MARKER, Profile};
use lt_core::{Backend, Event, MockBackend, RemotePath, TransferId};
use tokio::sync::broadcast;
use tokio::time::{Duration, Instant};

pub fn rp(parts: &[&str]) -> RemotePath {
    RemotePath {
        root: "desktop".to_owned(),
        components: parts.iter().map(|s| (*s).to_owned()).collect(),
    }
}

pub fn mock_in(dir: &Path, profile: Profile) -> MockBackend {
    let mut config = MockConfig::new(dir.join("sandbox"));
    config.profile = profile;
    MockBackend::new(config).expect("mock backend")
}

pub fn transfer_of(event: &Event) -> Option<TransferId> {
    match event {
        Event::TransferStarted { id, .. }
        | Event::TransferProgress { id, .. }
        | Event::TransferSuspended { id, .. }
        | Event::TransferResumed { id }
        | Event::FileCommitted { id, .. }
        | Event::TransferFinished { id, .. } => Some(*id),
        Event::Connection(_) | Event::RemoteChanged { .. } => None,
    }
}

/// Receives events for `id` (with the virtual time they arrived) until its
/// `TransferFinished`, then waits 30 s more and checks no second one arrives.
pub async fn run_to_end(
    rx: &mut broadcast::Receiver<Event>,
    id: TransferId,
) -> Vec<(Instant, Event)> {
    let mut seen = Vec::new();
    loop {
        let event = rx.recv().await.expect("event stream open, not lagged");
        if transfer_of(&event) == Some(id) {
            let finished = matches!(event, Event::TransferFinished { .. });
            seen.push((Instant::now(), event));
            if finished {
                break;
            }
        }
    }
    tokio::time::sleep(Duration::from_secs(30)).await;
    while let Ok(event) = rx.try_recv() {
        assert!(
            !(transfer_of(&event) == Some(id) && matches!(event, Event::TransferFinished { .. })),
            "second TransferFinished for {id:?}"
        );
    }
    seen
}

/// Every file under `dir` whose name marks it as an in-progress partial.
pub fn partials_under(dir: &Path) -> Vec<PathBuf> {
    walk_files(dir)
        .into_iter()
        .filter(|p| p.to_string_lossy().contains(PARTIAL_MARKER))
        .collect()
}

pub fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            out.extend(walk_files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// Names directly inside `components` according to the fixture file itself,
/// read independently of the mock's own loader.
pub fn fixture_names(components: &[&str]) -> BTreeSet<String> {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/mock-tree.json"),
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut level = json["entries"].as_array().unwrap().clone();
    for c in components {
        let node = level.iter().find(|n| n["name"] == *c).unwrap().clone();
        level.clone_from(node["children"].as_array().unwrap());
    }
    level
        .iter()
        .map(|n| n["name"].as_str().unwrap().to_owned())
        .collect()
}

pub fn fixture_size(components: &[&str]) -> u64 {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/mock-tree.json"),
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut level = json["entries"].as_array().unwrap().clone();
    let (last, parents) = components.split_last().unwrap();
    for c in parents {
        let node = level.iter().find(|n| n["name"] == *c).unwrap().clone();
        level.clone_from(node["children"].as_array().unwrap());
    }
    level.iter().find(|n| n["name"] == *last).unwrap()["size"]
        .as_u64()
        .unwrap()
}

/// Starts a transfer and returns its ID, subscribing first so no event is missed.
pub async fn start(
    backend: &impl Backend,
    req: lt_core::TransferRequest,
) -> (TransferId, broadcast::Receiver<Event>) {
    let rx = backend.events();
    let id = backend.start_transfer(req).await.expect("transfer starts");
    (id, rx)
}
