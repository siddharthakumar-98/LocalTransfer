//! Backend contract suite (ROADMAP.md §10).
//!
//! Each check is written once against the `Harness` trait. M1 runs it against
//! `MockBackend`; M2/M3 add a harness for the real node.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::{fixture_names, fixture_size, mock_in, partials_under, rp, run_to_end, start};
use lt_core::mock::Profile;
use lt_core::{
    Backend, BackendError, Direction, EntryKind, Event, MockBackend, MockControl, Outcome,
    RemotePath, TransferId, TransferRequest,
};
use tokio::time::Duration;

/// What the contract needs to know about one backend under test.
trait Harness {
    type B: Backend;
    fn backend(&self) -> &Self::B;
    /// Folder holding local files to send.
    fn local_dir(&self) -> &Path;
    /// Destination passed in `Get` requests.
    fn get_dest(&self) -> PathBuf;
    /// Where files received by a `Get` actually land.
    fn received_dir(&self) -> PathBuf;
    /// Names at the top of the peer's `desktop` root.
    fn expected_root(&self) -> BTreeSet<String>;
    /// A small peer file and its size.
    fn small_remote_file(&self) -> (RemotePath, u64);
    /// A peer file big enough to cancel mid-way.
    fn large_remote_file(&self) -> RemotePath;
}

struct MockHarness {
    _tmp: tempfile::TempDir,
    local: PathBuf,
    backend: MockBackend,
}

impl MockHarness {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let local = tmp.path().join("local");
        std::fs::create_dir(&local).unwrap();
        let backend = mock_in(tmp.path(), Profile::WifiGood);
        Self {
            local,
            backend,
            _tmp: tmp,
        }
    }
}

impl Harness for MockHarness {
    type B = MockBackend;
    fn backend(&self) -> &MockBackend {
        &self.backend
    }
    fn local_dir(&self) -> &Path {
        &self.local
    }
    fn get_dest(&self) -> PathBuf {
        self.local.join("ignored-by-mock")
    }
    fn received_dir(&self) -> PathBuf {
        self.backend.received_dir()
    }
    fn expected_root(&self) -> BTreeSet<String> {
        fixture_names(&[])
    }
    fn small_remote_file(&self) -> (RemotePath, u64) {
        (rp(&["Resume.pdf"]), fixture_size(&["Resume.pdf"]))
    }
    fn large_remote_file(&self) -> RemotePath {
        rp(&["Product Demo.mov"])
    }
}

fn finished(events: &[(tokio::time::Instant, Event)]) -> Vec<&Outcome> {
    events
        .iter()
        .filter_map(|(_, e)| match e {
            Event::TransferFinished { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect()
}

// ---- the contract ----

async fn lists_root(h: &impl Harness) {
    let entries = h.backend().list_dir(rp(&[])).await.unwrap();
    let names: BTreeSet<_> = entries.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, h.expected_root());
    assert!(entries.iter().any(|e| e.kind == EntryKind::Dir));
    assert!(
        entries
            .iter()
            .any(|e| e.kind == EntryKind::File && e.size > 0)
    );
}

async fn list_errors(h: &impl Harness) {
    let b = h.backend();
    assert_eq!(
        b.list_dir(rp(&["No Such Folder"])).await,
        Err(BackendError::NotFound)
    );
    assert_eq!(
        b.list_dir(rp(&[".."])).await,
        Err(BackendError::InvalidPath)
    );
    assert_eq!(
        b.list_dir(rp(&["a/b"])).await,
        Err(BackendError::InvalidPath)
    );
    let other_root = RemotePath {
        root: "etc".into(),
        components: vec![],
    };
    assert_eq!(b.list_dir(other_root).await, Err(BackendError::OutsideRoot));
    assert_eq!(
        b.cancel(TransferId(9_999)).await,
        Err(BackendError::NotFound)
    );
}

async fn get_reports_progress_and_finishes_once(h: &impl Harness) {
    let (remote, size) = h.small_remote_file();
    let (id, mut rx) = start(
        h.backend(),
        TransferRequest::Get {
            remote: vec![remote],
            dest: h.get_dest(),
        },
    )
    .await;
    let events = run_to_end(&mut rx, id).await;

    assert!(matches!(
        events.first().map(|(_, e)| e),
        Some(Event::TransferStarted { direction: Direction::Get, total_bytes, .. }) if *total_bytes == size
    ));
    let progress: Vec<_> = events
        .iter()
        .filter_map(|(at, e)| match e {
            Event::TransferProgress {
                bytes_done,
                total_bytes,
                ..
            } => Some((*at, *bytes_done, *total_bytes)),
            _ => None,
        })
        .collect();
    assert!(!progress.is_empty(), "no progress events");
    for pair in progress.windows(2) {
        assert!(pair[1].1 >= pair[0].1, "progress went backwards");
        assert!(
            pair[1].0 - pair[0].0 >= Duration::from_millis(100),
            "more than 10 Hz"
        );
    }
    assert_eq!(
        progress.last().map(|p| (p.1, p.2)),
        Some((size, size)),
        "reaches 100%"
    );
    assert_eq!(finished(&events), vec![&Outcome::Completed]);

    let committed: Vec<_> = events
        .iter()
        .filter_map(|(_, e)| match e {
            Event::FileCommitted { final_path, .. } => Some(PathBuf::from(final_path)),
            _ => None,
        })
        .collect();
    assert_eq!(committed.len(), 1);
    assert!(committed[0].starts_with(h.received_dir()));
    assert_eq!(std::fs::metadata(&committed[0]).unwrap().len(), size);
    assert!(
        partials_under(&h.received_dir()).is_empty(),
        "partial file left behind"
    );
    assert!(
        !h.get_dest().exists(),
        "Get must not write to the requested destination"
    );
}

async fn send_adds_file_and_leaves_source_untouched(h: &impl Harness) {
    let src = h.local_dir().join("report.pdf");
    let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&src, &body).unwrap();
    let before = std::fs::metadata(&src).unwrap().modified().unwrap();

    let (id, mut rx) = start(
        h.backend(),
        TransferRequest::Send {
            local: vec![src.clone()],
            dest: rp(&[]),
        },
    )
    .await;
    let events = run_to_end(&mut rx, id).await;
    assert_eq!(finished(&events), vec![&Outcome::Completed]);

    let listing = h.backend().list_dir(rp(&[])).await.unwrap();
    let entry = listing
        .iter()
        .find(|e| e.name == "report.pdf")
        .expect("sent file listed");
    assert_eq!(entry.size, body.len() as u64);
    assert_eq!(std::fs::read(&src).unwrap(), body, "source bytes unchanged");
    assert_eq!(std::fs::metadata(&src).unwrap().modified().unwrap(), before);
}

async fn cancel_removes_partial(h: &impl Harness) {
    let (id, mut rx) = start(
        h.backend(),
        TransferRequest::Get {
            remote: vec![h.large_remote_file()],
            dest: h.get_dest(),
        },
    )
    .await;
    // Wait for some progress, then cancel.
    loop {
        if let Event::TransferProgress { id: pid, .. } = rx.recv().await.unwrap()
            && pid == id
        {
            break;
        }
    }
    assert!(
        !partials_under(&h.received_dir()).is_empty(),
        "partial exists mid-transfer"
    );
    h.backend().cancel(id).await.unwrap();
    let events = run_to_end(&mut rx, id).await;
    assert_eq!(finished(&events), vec![&Outcome::Cancelled]);
    assert!(
        partials_under(&h.received_dir()).is_empty(),
        "partial removed"
    );
    assert!(
        common::walk_files(&h.received_dir()).is_empty(),
        "nothing committed"
    );
}

async fn clashes_are_numbered_never_overwritten(h: &impl Harness) {
    let (remote, _) = h.small_remote_file();
    let mut finals = Vec::new();
    for _ in 0..2 {
        let (id, mut rx) = start(
            h.backend(),
            TransferRequest::Get {
                remote: vec![remote.clone()],
                dest: h.get_dest(),
            },
        )
        .await;
        for (_, e) in run_to_end(&mut rx, id).await {
            if let Event::FileCommitted {
                final_path,
                renamed,
                ..
            } = e
            {
                finals.push((final_path, renamed));
            }
        }
    }
    let name = remote.components.last().unwrap();
    let (stem, ext) = name.rsplit_once('.').unwrap();
    assert!(finals[0].0.ends_with(&format!("/{name}")) && !finals[0].1);
    assert!(finals[1].0.ends_with(&format!("/{stem} (1).{ext}")) && finals[1].1);

    // Same for sends: the peer keeps both.
    let src = h.local_dir().join("dup.txt");
    std::fs::write(&src, b"dup").unwrap();
    for _ in 0..2 {
        let (id, mut rx) = start(
            h.backend(),
            TransferRequest::Send {
                local: vec![src.clone()],
                dest: rp(&[]),
            },
        )
        .await;
        run_to_end(&mut rx, id).await;
    }
    let names: BTreeSet<_> = h
        .backend()
        .list_dir(rp(&[]))
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(names.contains("dup.txt") && names.contains("dup (1).txt"));
}

// ---- run the contract against the mock ----

#[tokio::test(start_paused = true)]
async fn mock_lists_root() {
    lists_root(&MockHarness::new()).await;
}

#[tokio::test(start_paused = true)]
async fn mock_list_errors() {
    list_errors(&MockHarness::new()).await;
}

#[tokio::test(start_paused = true)]
async fn mock_get_progress_and_single_finish() {
    get_reports_progress_and_finishes_once(&MockHarness::new()).await;
}

#[tokio::test(start_paused = true)]
async fn mock_send_leaves_source_untouched() {
    send_adds_file_and_leaves_source_untouched(&MockHarness::new()).await;
}

#[tokio::test(start_paused = true)]
async fn mock_cancel_removes_partial() {
    cancel_removes_partial(&MockHarness::new()).await;
}

#[tokio::test(start_paused = true)]
async fn mock_clashes_numbered() {
    clashes_are_numbered_never_overwritten(&MockHarness::new()).await;
}
