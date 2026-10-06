//! Behaviour only the mock peer has: link simulation, drops and sandboxing.

mod common;

use std::collections::{BTreeSet, HashMap};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use common::{mock_in, partials_under, rp, run_to_end, start, walk_files};
use lt_core::mock::{BLIP_DURATION, DropKind, MockLink, Profile, RECONNECT_GRACE};
use lt_core::{
    Backend, BackendError, ConnectionState, Event, LinkQuality, MockControl, Outcome, TransferId,
    TransferRequest,
};
use proptest::prelude::*;
use tokio::time::{Duration, Instant};

fn get(paths: &[&[&str]]) -> TransferRequest {
    TransferRequest::Get {
        remote: paths.iter().map(|p| rp(p)).collect(),
        dest: PathBuf::from("/nonexistent/ignored-by-mock"),
    }
}

fn outcome_of(events: &[(Instant, Event)]) -> Outcome {
    events
        .iter()
        .find_map(|(_, e)| match e {
            Event::TransferFinished { outcome, .. } => Some(outcome.clone()),
            _ => None,
        })
        .expect("finished")
}

#[tokio::test(start_paused = true)]
async fn blip_suspends_then_resumes() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = mock_in(tmp.path(), Profile::WifiGood);
    let (id, mut rx) = start(&mock, get(&[&["Installer.dmg"]])).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    mock.drop_connection(DropKind::Blip);
    assert_eq!(mock.connection(), ConnectionState::Connecting);

    let events = run_to_end(&mut rx, id).await;
    let kinds: Vec<_> = events
        .iter()
        .filter_map(|(at, e)| match e {
            Event::TransferSuspended { reason, .. } => {
                Some(("suspended", *at, Some(reason.clone())))
            }
            Event::TransferResumed { .. } => Some(("resumed", *at, None)),
            _ => None,
        })
        .collect();
    assert_eq!(kinds.len(), 2, "{kinds:?}");
    assert_eq!(kinds[0].0, "suspended");
    assert_eq!(kinds[0].2, Some(BackendError::ConnectionLost));
    assert_eq!(kinds[1].0, "resumed");
    assert!(kinds[1].1 - kinds[0].1 <= BLIP_DURATION + Duration::from_millis(200));
    assert_eq!(outcome_of(&events), Outcome::Completed);
    assert!(matches!(
        mock.connection(),
        ConnectionState::Connected { .. }
    ));
}

#[tokio::test(start_paused = true)]
async fn permanent_drop_fails_after_grace_and_cleans_up() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = mock_in(tmp.path(), Profile::WifiGood);
    let (id, mut rx) = start(&mock, get(&[&["Installer.dmg"]])).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let dropped_at = Instant::now();
    mock.drop_connection(DropKind::Permanent);
    assert!(matches!(
        mock.connection(),
        ConnectionState::Disconnected { .. }
    ));

    let events = run_to_end(&mut rx, id).await;
    let failed_at = events
        .iter()
        .find_map(|(at, e)| matches!(e, Event::TransferFinished { .. }).then_some(*at))
        .unwrap();
    assert_eq!(
        outcome_of(&events),
        Outcome::Failed(BackendError::ConnectionLost)
    );
    assert!(failed_at - dropped_at >= RECONNECT_GRACE);
    assert!(failed_at - dropped_at < RECONNECT_GRACE + Duration::from_millis(500));
    assert!(
        partials_under(&mock.received_dir()).is_empty(),
        "partial file left behind"
    );
    assert!(
        walk_files(&mock.received_dir()).is_empty(),
        "file left in the sandbox"
    );

    // While down, new work is refused; after reconnecting it works again.
    assert_eq!(
        mock.list_dir(rp(&[])).await,
        Err(BackendError::NotConnected)
    );
    assert_eq!(
        mock.start_transfer(get(&[&["todo.txt"]])).await,
        Err(BackendError::NotConnected)
    );
    mock.set_link(MockLink::Good);
    assert!(mock.list_dir(rp(&[])).await.is_ok());
}

#[tokio::test(start_paused = true)]
async fn drop_at_fifty_percent_fails_mid_transfer() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = mock_in(tmp.path(), Profile::Fast);
    mock.drop_at(50);
    let (id, mut rx) = start(&mock, get(&[&["Wallpaper.png"]])).await;
    let events = run_to_end(&mut rx, id).await;
    let (max_done, total) = events
        .iter()
        .filter_map(|(_, e)| match e {
            Event::TransferProgress {
                bytes_done,
                total_bytes,
                ..
            } => Some((*bytes_done, *total_bytes)),
            _ => None,
        })
        .max()
        .unwrap();
    assert_eq!(max_done, total / 2, "stops exactly at 50%");
    assert_eq!(
        outcome_of(&events),
        Outcome::Failed(BackendError::ConnectionLost)
    );
    assert!(
        walk_files(&mock.received_dir()).is_empty(),
        "no file or partial left"
    );

    // The trigger is one-shot: the next transfer completes.
    mock.set_link(MockLink::Good);
    let (id, mut rx) = start(&mock, get(&[&["Wallpaper.png"]])).await;
    assert_eq!(
        outcome_of(&run_to_end(&mut rx, id).await),
        Outcome::Completed
    );
}

#[tokio::test(start_paused = true)]
async fn connection_events_follow_link_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = mock_in(tmp.path(), Profile::WifiGood);
    let mut rx = mock.events();
    mock.set_link(MockLink::Weak);
    mock.drop_connection(DropKind::Blip);
    tokio::time::sleep(BLIP_DURATION + Duration::from_millis(10)).await;
    let mut states = Vec::new();
    while let Ok(Event::Connection(state)) = rx.try_recv() {
        states.push(state);
    }
    assert_eq!(
        states,
        vec![
            ConnectionState::Connected {
                quality: LinkQuality::Weak
            },
            ConnectionState::Connecting,
            ConnectionState::Connected {
                quality: LinkQuality::Weak
            },
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn weak_link_is_slower_than_good() {
    async fn duration(link: MockLink) -> Duration {
        let tmp = tempfile::tempdir().unwrap();
        let mock = mock_in(tmp.path(), Profile::WifiGood);
        mock.set_link(link);
        let started = Instant::now();
        let (id, mut rx) = start(&mock, get(&[&["Slides.key"]])).await;
        let events = run_to_end(&mut rx, id).await;
        assert_eq!(outcome_of(&events), Outcome::Completed);
        events.last().unwrap().0 - started
    }
    let good = duration(MockLink::Good).await;
    let weak = duration(MockLink::Weak).await;
    assert!(weak > good * 10, "good {good:?}, weak {weak:?}");
}

#[tokio::test(start_paused = true)]
async fn large_files_are_sparse_and_content_is_deterministic() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = mock_in(tmp.path(), Profile::Fast);
    let (id, mut rx) = start(&mock, get(&[&["Product Demo.mov"], &["Meeting Notes.md"]])).await;
    let events = run_to_end(&mut rx, id).await;
    assert_eq!(outcome_of(&events), Outcome::Completed);
    let movie = std::fs::metadata(mock.received_dir().join("Product Demo.mov")).unwrap();
    assert_eq!(movie.len(), common::fixture_size(&["Product Demo.mov"]));
    assert!(
        movie.blocks() * 512 < 1 << 20,
        "sparse: {} blocks",
        movie.blocks()
    );

    let other = tempfile::tempdir().unwrap();
    let again = mock_in(other.path(), Profile::Fast);
    let (id, mut rx) = start(&again, get(&[&["Meeting Notes.md"]])).await;
    run_to_end(&mut rx, id).await;
    let a = std::fs::read(mock.received_dir().join("Meeting Notes.md")).unwrap();
    let b = std::fs::read(again.received_dir().join("Meeting Notes.md")).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.len() as u64, common::fixture_size(&["Meeting Notes.md"]));
}

#[tokio::test(start_paused = true)]
async fn get_folder_keeps_structure_and_renames_on_clash() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = mock_in(tmp.path(), Profile::Fast);
    for _ in 0..2 {
        let (id, mut rx) = start(&mock, get(&[&["Invoices 2026"], &["Empty Folder"]])).await;
        assert_eq!(
            outcome_of(&run_to_end(&mut rx, id).await),
            Outcome::Completed
        );
    }
    let r = mock.received_dir();
    assert!(r.join("Invoices 2026/Receipts/Receipt 01.pdf").is_file());
    assert!(
        r.join("Invoices 2026 (1)/Receipts/Receipt 01.pdf")
            .is_file()
    );
    assert!(r.join("Empty Folder").is_dir() && r.join("Empty Folder (1)").is_dir());
    mock.reset_sandbox().unwrap();
    assert!(walk_files(&r).is_empty(), "reset should empty Received/");
}

// ---- property test: whatever happens, writes stay in the sandbox ----

#[derive(Debug, Clone)]
enum Op {
    Send(usize),
    Get(usize),
    Cancel(usize),
    Blip,
    Drop,
    Reconnect,
    Wait(u64),
}

const GET_TARGETS: &[&[&str]] = &[
    &["Meeting Notes.md"],
    &["empty.txt"],
    &["Projects", "LocalTransfer"],
    &["Invoices 2026", "Receipts"],
    &["Empty Folder"],
    &["Resume.pdf"],
];

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..3usize).prop_map(Op::Send),
        (0..GET_TARGETS.len()).prop_map(Op::Get),
        (0..6usize).prop_map(Op::Cancel),
        Just(Op::Blip),
        Just(Op::Drop),
        Just(Op::Reconnect),
        (0..4_000u64).prop_map(Op::Wait),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn writes_stay_in_sandbox_and_sources_are_untouched(
        ops in prop::collection::vec(op(), 1..12),
        bodies in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..4096), 3),
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap();
        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            let src = tmp.path().join("src");
            let decoy = tmp.path().join("decoy");
            std::fs::create_dir(&src).unwrap();
            std::fs::create_dir(&decoy).unwrap();
            let sources: Vec<PathBuf> = bodies
                .iter()
                .enumerate()
                .map(|(i, body)| {
                    let p = src.join(format!("file{i}.bin"));
                    std::fs::write(&p, body).unwrap();
                    p
                })
                .collect();

            let mock = mock_in(tmp.path(), Profile::WifiGood);
            let log: Arc<Mutex<Vec<Event>>> = Arc::default();
            let mut rx = mock.events();
            let sink = Arc::clone(&log);
            tokio::spawn(async move {
                while let Ok(e) = rx.recv().await {
                    sink.lock().unwrap().push(e);
                }
            });

            let mut started: Vec<TransferId> = Vec::new();
            for op in &ops {
                match op {
                    Op::Send(i) => {
                        let req = TransferRequest::Send { local: vec![sources[*i].clone()], dest: rp(&[]) };
                        if let Ok(id) = mock.start_transfer(req).await { started.push(id); }
                    }
                    Op::Get(i) => {
                        let req = TransferRequest::Get { remote: vec![rp(GET_TARGETS[*i])], dest: decoy.clone() };
                        if let Ok(id) = mock.start_transfer(req).await { started.push(id); }
                    }
                    Op::Cancel(k) => {
                        if let Some(id) = started.get(*k) { let _ = mock.cancel(*id).await; }
                    }
                    Op::Blip => mock.drop_connection(DropKind::Blip),
                    Op::Drop => mock.drop_connection(DropKind::Permanent),
                    Op::Reconnect => mock.set_link(MockLink::Good),
                    Op::Wait(ms) => tokio::time::sleep(Duration::from_millis(*ms)).await,
                }
            }
            // Long enough for every transfer to finish, fail or be cancelled.
            tokio::time::sleep(Duration::from_secs(120)).await;

            // Nothing outside the sandbox changed.
            let top: BTreeSet<_> = std::fs::read_dir(tmp.path()).unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
            assert_eq!(top, BTreeSet::from(["decoy".into(), "sandbox".into(), "src".into()]));
            assert!(walk_files(&decoy).is_empty(), "Get wrote to the requested destination");
            for (path, body) in sources.iter().zip(&bodies) {
                assert_eq!(&std::fs::read(path).unwrap(), body, "source modified");
            }
            assert_eq!(walk_files(&src).len(), sources.len());

            // Inside the sandbox: only Received/, no partials, no overwrites.
            let sandbox = mock.sandbox_root();
            for f in walk_files(&sandbox) {
                assert!(f.starts_with(mock.received_dir()), "{f:?} outside Received/");
            }
            assert!(partials_under(&sandbox).is_empty(), "partial file left behind");

            let log = log.lock().unwrap();
            let mut finished: HashMap<TransferId, usize> = HashMap::new();
            let mut committed = BTreeSet::new();
            for e in log.iter() {
                match e {
                    Event::TransferFinished { id, .. } => *finished.entry(*id).or_default() += 1,
                    Event::FileCommitted { final_path, .. } => {
                        assert!(committed.insert(final_path.clone()), "{final_path} committed twice");
                    }
                    _ => {}
                }
            }
            for id in &started {
                assert_eq!(finished.get(id), Some(&1), "{id:?} must finish exactly once");
            }
        });
    }
}
