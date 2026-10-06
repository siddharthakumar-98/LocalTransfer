//! A mock peer that pretends to be the other Mac (ROADMAP.md §5, "Mock peer").
//!
//! It serves a fake file tree, simulates latency and throughput on the tokio
//! clock (so tests can run with time paused), and can drop the connection on
//! demand. It writes only inside its sandbox folder, never overwrites, and
//! never modifies the local files it "sends".

mod sandbox;
mod sim;
mod tree;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, watch};
use tokio::time::{Instant, sleep, sleep_until};

use crate::backend::{
    Backend, BackendError, ConnectionState, Direction, Entry, Event, LinkQuality, Outcome,
    PeerInfo, RemotePath, TransferId, TransferRequest,
};
use crate::naming::{validate_component, validate_components};
use sandbox::{Partial, Sandbox};
use sim::{SplitMix64, TICK, fnv1a};
use tree::FakeTree;

pub use sandbox::PARTIAL_MARKER;
pub use sim::Profile;

/// How long a Blip keeps the peer away.
pub const BLIP_DURATION: Duration = Duration::from_secs(3);

/// How long a transfer stays suspended before failing with `ConnectionLost`.
pub const RECONNECT_GRACE: Duration = Duration::from_secs(10);

/// Files larger than this are received as sparse files instead of being filled.
pub const SPARSE_THRESHOLD: u64 = 64 * 1024 * 1024;

/// Generated content is written in pieces of this size.
const WRITE_CHUNK: usize = 64 * 1024;

/// Link setting for [`MockControl::set_link`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockLink {
    Good,
    Weak,
    Down,
}

/// How [`MockControl::drop_connection`] drops the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropKind {
    /// Gone for [`BLIP_DURATION`], then back. Transfers suspend and resume.
    Blip,
    /// Gone until [`MockControl::set_link`] brings it back. Transfers fail
    /// after [`RECONNECT_GRACE`].
    Permanent,
}

/// Debug controls that only the mock has. Not part of [`Backend`].
pub trait MockControl: Send + Sync {
    fn set_link(&self, link: MockLink);
    fn set_profile(&self, profile: Profile);
    fn drop_connection(&self, kind: DropKind);
    /// Drops the link permanently once the next transfer reaches `percent`
    /// (clamped to 0–100) of its bytes.
    fn drop_at(&self, percent: u8);
    /// Deletes everything the mock has received.
    ///
    /// # Errors
    /// [`BackendError::Io`] if the sandbox can't be emptied.
    fn reset_sandbox(&self) -> Result<(), BackendError>;
    /// The sandbox root; nothing outside it is ever written.
    fn sandbox_root(&self) -> PathBuf;
    /// Where received files land: `<sandbox root>/Received`.
    fn received_dir(&self) -> PathBuf;
}

/// Settings for a [`MockBackend`].
#[derive(Debug, Clone)]
pub struct MockConfig {
    pub sandbox_root: PathBuf,
    pub profile: Profile,
    pub seed: u64,
    pub peer: PeerInfo,
}

impl MockConfig {
    /// Defaults: Wi-Fi-good throughput, seed 1, a peer called "Mock Mac"
    /// (deliberately not a real model name, so it can't be mistaken for a real Mac).
    #[must_use]
    pub fn new(sandbox_root: impl Into<PathBuf>) -> Self {
        Self {
            sandbox_root: sandbox_root.into(),
            profile: Profile::WifiGood,
            seed: 1,
            peer: PeerInfo {
                alias: "mock".to_owned(),
                device_name: "Mock Mac".to_owned(),
                model: None,
                device_id: "MOCK-MOCK-MOCK-MOCK-MOCK".to_owned(),
            },
        }
    }
}

/// `~/Library/Application Support/LocalTransfer/MockSandbox`, if `HOME` is set.
#[must_use]
pub fn default_sandbox_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        Path::new(&home)
            .join("Library/Application Support/LocalTransfer")
            .join("MockSandbox"),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    Up(LinkQuality),
    /// A Blip in progress.
    Reconnecting,
    Down,
}

impl Link {
    fn is_up(self) -> bool {
        matches!(self, Self::Up(_))
    }
}

/// The mock peer. Create it inside a tokio runtime (or pass a [`Handle`]).
pub struct MockBackend {
    shared: Arc<Shared>,
}

struct Shared {
    peer: PeerInfo,
    seed: u64,
    handle: Handle,
    sandbox: Sandbox,
    tree: Mutex<FakeTree>,
    link: watch::Sender<Link>,
    /// Bumped on every manual link change, so a pending Blip restore can tell
    /// it has been overtaken.
    link_generation: AtomicU64,
    profile: Mutex<Profile>,
    rng: Mutex<SplitMix64>,
    events: broadcast::Sender<Event>,
    transfers: Mutex<HashMap<TransferId, watch::Sender<bool>>>,
    next_id: AtomicU64,
    drop_at: Mutex<Option<u8>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding one of these locks can't leave the data half-updated
    // in a way that matters for a mock, so recover from poisoning.
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn io_err(e: &std::io::Error) -> BackendError {
    BackendError::Io(e.to_string())
}

impl MockBackend {
    /// Creates the mock on the current tokio runtime.
    ///
    /// # Errors
    /// [`BackendError::Internal`] outside a tokio runtime;
    /// [`BackendError::Io`] if the sandbox can't be created.
    pub fn new(config: MockConfig) -> Result<Self, BackendError> {
        let handle = Handle::try_current()
            .map_err(|_| BackendError::Internal("MockBackend needs a tokio runtime".into()))?;
        Self::with_handle(config, handle)
    }

    /// Creates the mock, spawning its tasks on `handle`.
    ///
    /// # Errors
    /// [`BackendError::Io`] if the sandbox can't be created.
    pub fn with_handle(config: MockConfig, handle: Handle) -> Result<Self, BackendError> {
        let sandbox = Sandbox::open(&config.sandbox_root).map_err(|e| io_err(&e))?;
        let (events, _) = broadcast::channel(1024);
        let (link, _) = watch::channel(Link::Up(LinkQuality::Good));
        Ok(Self {
            shared: Arc::new(Shared {
                peer: config.peer,
                seed: config.seed,
                handle,
                sandbox,
                tree: Mutex::new(FakeTree::from_fixture()),
                link,
                link_generation: AtomicU64::new(0),
                profile: Mutex::new(config.profile),
                rng: Mutex::new(SplitMix64::new(config.seed)),
                events,
                transfers: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
                drop_at: Mutex::new(None),
            }),
        })
    }
}

impl Shared {
    fn emit(&self, event: Event) {
        // No subscribers is fine.
        let _ = self.events.send(event);
    }

    fn connection(&self) -> ConnectionState {
        match *self.link.borrow() {
            Link::Up(quality) => ConnectionState::Connected { quality },
            Link::Reconnecting => ConnectionState::Connecting,
            Link::Down => ConnectionState::Disconnected {
                reason: format!("Lost connection to {}", self.peer.device_name),
            },
        }
    }

    fn set_link(&self, link: Link) {
        let changed = self.link.send_if_modified(|current| {
            let changed = *current != link;
            *current = link;
            changed
        });
        if changed {
            self.emit(Event::Connection(self.connection()));
        }
    }

    fn model(&self) -> sim::LinkModel {
        match *self.link.borrow() {
            Link::Up(LinkQuality::Weak) => Profile::Weak.model(),
            _ => lock(&self.profile).model(),
        }
    }

    fn require_up(&self) -> Result<(), BackendError> {
        if self.link.borrow().is_up() {
            Ok(())
        } else {
            Err(BackendError::NotConnected)
        }
    }

    fn check_root(&self, path: &RemotePath) -> Result<(), BackendError> {
        if path.root != lock(&self.tree).root_name {
            return Err(BackendError::OutsideRoot);
        }
        validate_components(&path.components)
    }

    fn display_remote(components: &[String]) -> String {
        let mut s = "~/Desktop".to_owned();
        for c in components {
            s.push('/');
            s.push_str(c);
        }
        s
    }
}

/// One file of a planned transfer.
#[derive(Debug, Clone)]
struct PlanFile {
    /// Send: path in the fake tree. Get: path under `Received/`.
    dest: Vec<String>,
    size: u64,
    /// Send only.
    modified: SystemTime,
    /// Get only: seeds the generated content.
    content_seed: u64,
}

#[derive(Debug)]
struct Plan {
    direction: Direction,
    files: Vec<PlanFile>,
    /// Folders to create even if they end up empty.
    dirs: Vec<Vec<String>>,
    total_bytes: u64,
    items: u32,
    drop_at: Option<u8>,
    /// Send only: the peer folder to tell the UI to refresh.
    notify: Option<RemotePath>,
}

enum Stop {
    Cancelled,
    Failed(BackendError),
}

impl MockBackend {
    fn plan_send(&self, local: &[PathBuf], dest: &RemotePath) -> Result<Plan, BackendError> {
        let s = &self.shared;
        s.check_root(dest)?;
        let tree = lock(&s.tree);
        if !tree.is_dir(&dest.components) {
            return Err(BackendError::NotFound);
        }
        let mut plan = Plan {
            direction: Direction::Send,
            files: Vec::new(),
            dirs: Vec::new(),
            total_bytes: 0,
            items: 0,
            drop_at: None,
            notify: None,
        };
        for path in local {
            // Metadata only: the mock never reads, moves or modifies local files.
            let meta = std::fs::symlink_metadata(path).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => BackendError::NotFound,
                std::io::ErrorKind::PermissionDenied => BackendError::PermissionDenied,
                _ => io_err(&e),
            })?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or(BackendError::InvalidPath)?;
            validate_component(name)?;
            if meta.file_type().is_symlink() {
                return Err(BackendError::InvalidPath);
            }
            let (top, _) = tree.unique_name(&dest.components, name);
            let mut base = dest.components.clone();
            base.push(top);
            if meta.is_dir() {
                walk_local(path, &base, &mut plan)?;
            } else {
                plan.total_bytes += meta.len();
                plan.files.push(PlanFile {
                    dest: base,
                    size: meta.len(),
                    modified: meta.modified().unwrap_or_else(|_| SystemTime::now()),
                    content_seed: 0,
                });
            }
            plan.items += 1;
        }
        plan.notify = Some(dest.clone());
        Ok(plan)
    }

    fn plan_get(&self, remote: &[RemotePath]) -> Result<Plan, BackendError> {
        let s = &self.shared;
        // Validate first: check_root takes the tree lock itself.
        for path in remote {
            s.check_root(path)?;
        }
        let tree = lock(&s.tree);
        let mut plan = Plan {
            direction: Direction::Get,
            files: Vec::new(),
            dirs: Vec::new(),
            total_bytes: 0,
            items: 0,
            drop_at: None,
            notify: None,
        };
        for path in remote {
            let (files, dirs) = tree.walk(&path.components)?;
            // A folder gets a fresh top-level name now ("Photos (1)"); files
            // are numbered at commit time.
            let is_dir = !dirs.is_empty();
            let top = if is_dir {
                s.sandbox
                    .unique_top_level(&path.components[path.components.len() - 1])
                    .map_err(|e| io_err(&e))?
            } else {
                path.components[path.components.len() - 1].clone()
            };
            let rename = |rel: &[String]| -> Vec<String> {
                let mut out = vec![top.clone()];
                out.extend_from_slice(&rel[1..]);
                out
            };
            let parent = &path.components[..path.components.len() - 1];
            for f in files {
                let mut full = parent.to_vec();
                full.extend_from_slice(&f.rel);
                plan.total_bytes += f.size;
                plan.files.push(PlanFile {
                    dest: rename(&f.rel),
                    size: f.size,
                    modified: SystemTime::now(),
                    content_seed: s.seed ^ fnv1a(&full),
                });
            }
            plan.dirs.extend(dirs.iter().map(|d| rename(d)));
            plan.items += 1;
        }
        Ok(plan)
    }

    async fn run(
        shared: Arc<Shared>,
        id: TransferId,
        plan: Plan,
        mut cancel: watch::Receiver<bool>,
    ) {
        let mut current: Option<Partial> = None;
        let result = transfer(&shared, id, &plan, &mut cancel, &mut current).await;
        let outcome = match result {
            Ok(()) => Outcome::Completed,
            Err(stop) => {
                if let Some(partial) = current.take() {
                    let _ = partial.discard();
                }
                if plan.direction == Direction::Get {
                    // Leave no empty folders behind; committed files stay.
                    for dir in plan.dirs.iter().rev() {
                        shared.sandbox.remove_dir_if_empty(dir);
                    }
                }
                match stop {
                    Stop::Cancelled => Outcome::Cancelled,
                    Stop::Failed(e) => Outcome::Failed(e),
                }
            }
        };
        lock(&shared.transfers).remove(&id);
        shared.emit(Event::TransferFinished { id, outcome });
    }
}

fn walk_local(dir: &Path, base: &[String], plan: &mut Plan) -> Result<(), BackendError> {
    plan.dirs.push(base.to_vec());
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| io_err(&e))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let Ok(meta) = entry.metadata() else { continue };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if meta.file_type().is_symlink() || validate_component(&name).is_err() {
            continue; // Never follow symlinks; skip names the peer would reject.
        }
        let mut rel = base.to_vec();
        rel.push(name);
        if meta.is_dir() {
            walk_local(&entry.path(), &rel, plan)?;
        } else {
            plan.total_bytes += meta.len();
            plan.files.push(PlanFile {
                dest: rel,
                size: meta.len(),
                modified: meta.modified().unwrap_or_else(|_| SystemTime::now()),
                content_seed: 0,
            });
        }
    }
    Ok(())
}

async fn pause(dur: Duration, cancel: &mut watch::Receiver<bool>) -> Result<(), Stop> {
    if *cancel.borrow() {
        return Err(Stop::Cancelled);
    }
    tokio::select! {
        () = sleep(dur) => Ok(()),
        _ = cancel.changed() => Err(Stop::Cancelled),
    }
}

/// Returns once the link is up. Around an outage it emits `TransferSuspended`
/// and `TransferResumed`; after [`RECONNECT_GRACE`] it gives up.
async fn await_link(
    shared: &Shared,
    id: TransferId,
    cancel: &mut watch::Receiver<bool>,
) -> Result<(), Stop> {
    let mut link = shared.link.subscribe();
    if link.borrow_and_update().is_up() {
        return Ok(());
    }
    shared.emit(Event::TransferSuspended {
        id,
        reason: BackendError::ConnectionLost,
    });
    let deadline = Instant::now() + RECONNECT_GRACE;
    loop {
        if *cancel.borrow() {
            return Err(Stop::Cancelled);
        }
        tokio::select! {
            () = sleep_until(deadline) => return Err(Stop::Failed(BackendError::ConnectionLost)),
            _ = cancel.changed() => return Err(Stop::Cancelled),
            changed = link.changed() => {
                if changed.is_err() {
                    return Err(Stop::Failed(BackendError::ConnectionLost));
                }
                if link.borrow_and_update().is_up() {
                    shared.emit(Event::TransferResumed { id });
                    return Ok(());
                }
            }
        }
    }
}

async fn transfer(
    shared: &Shared,
    id: TransferId,
    plan: &Plan,
    cancel: &mut watch::Receiver<bool>,
    current: &mut Option<Partial>,
) -> Result<(), Stop> {
    shared.emit(Event::TransferStarted {
        id,
        direction: plan.direction,
        items: plan.items,
        total_bytes: plan.total_bytes,
    });
    let total = plan.total_bytes;
    let drop_threshold = plan
        .drop_at
        .map(|pct| u64::try_from(u128::from(total) * u128::from(pct) / 100).unwrap_or(total));
    let mut dropped = false;

    await_link(shared, id, cancel).await?;
    pause(shared.model().rtt, cancel).await?;

    let (mut done, mut file_idx, mut file_done, mut rate) = (0u64, 0usize, 0u64, 0u64);
    let mut generator = SplitMix64::new(0);
    let mut buf = vec![0u8; WRITE_CHUNK];

    loop {
        await_link(shared, id, cancel).await?;
        pause(TICK, cancel).await?;
        await_link(shared, id, cancel).await?;

        let mut budget = shared.model().bytes_this_tick(&mut lock(&shared.rng));
        if let Some(threshold) = drop_threshold.filter(|_| !dropped) {
            // Stop exactly at the threshold so the drop lands mid-transfer.
            budget = budget.min(threshold.saturating_sub(done));
        }
        let tick_start = done;

        while let Some(file) = plan.files.get(file_idx) {
            if plan.direction == Direction::Get && current.is_none() {
                let (name, parent) = file.dest.split_last().expect("planned paths are non-empty");
                *current = Some(
                    shared
                        .sandbox
                        .create_partial(parent, name, id)
                        .map_err(|e| Stop::Failed(io_err(&e)))?,
                );
                generator = SplitMix64::new(file.content_seed);
            }
            let take = budget.min(file.size - file_done);
            if take > 0 {
                if let Some(partial) = current.as_mut().filter(|_| file.size <= SPARSE_THRESHOLD) {
                    let mut left = take;
                    while left > 0 {
                        let n = usize::try_from(left).map_or(WRITE_CHUNK, |l| l.min(WRITE_CHUNK));
                        generator.fill(&mut buf[..n]);
                        partial
                            .write_all(&buf[..n])
                            .map_err(|e| Stop::Failed(io_err(&e)))?;
                        left -= n as u64;
                    }
                }
                file_done += take;
                done += take;
                budget -= take;
            }
            if file_done < file.size {
                break; // Out of budget for this tick.
            }
            commit(shared, id, plan, file, current)?;
            file_idx += 1;
            file_done = 0;
        }

        // Smoothed rate: 70% previous, 30% this tick (ticks are 1/10 s).
        rate = (rate * 7 + (done - tick_start) * 10 * 3) / 10;
        let current_file = plan
            .files
            .get(file_idx.min(plan.files.len().saturating_sub(1)))
            .and_then(|f| f.dest.last().cloned())
            .unwrap_or_default();
        shared.emit(Event::TransferProgress {
            id,
            bytes_done: done,
            total_bytes: total,
            bytes_per_sec: rate,
            current_file,
        });

        if let Some(threshold) = drop_threshold
            && !dropped
            && done >= threshold
            && file_idx < plan.files.len()
        {
            dropped = true;
            shared.link_generation.fetch_add(1, Ordering::SeqCst);
            shared.set_link(Link::Down);
        }
        if file_idx >= plan.files.len() {
            break;
        }
    }

    finish_dirs(shared, plan);
    if let Some(path) = plan.notify.clone() {
        shared.emit(Event::RemoteChanged { path });
    }
    Ok(())
}

fn commit(
    shared: &Shared,
    id: TransferId,
    plan: &Plan,
    file: &PlanFile,
    current: &mut Option<Partial>,
) -> Result<(), Stop> {
    let (final_path, renamed) = match plan.direction {
        Direction::Get => {
            let partial = current.take().expect("a Get always has an open partial");
            if file.size > SPARSE_THRESHOLD {
                partial
                    .set_len(file.size)
                    .map_err(|e| Stop::Failed(io_err(&e)))?;
            }
            let (path, renamed) = partial.commit().map_err(|e| Stop::Failed(io_err(&e)))?;
            (path.display().to_string(), renamed)
        }
        Direction::Send => {
            let (name, parent) = file.dest.split_last().expect("planned paths are non-empty");
            let (final_name, renamed) =
                lock(&shared.tree).insert_file(parent, name, file.size, file.modified);
            let mut path = parent.to_vec();
            path.push(final_name);
            (Shared::display_remote(&path), renamed)
        }
    };
    shared.emit(Event::FileCommitted {
        id,
        final_path,
        renamed,
    });
    Ok(())
}

fn finish_dirs(shared: &Shared, plan: &Plan) {
    for dir in &plan.dirs {
        match plan.direction {
            Direction::Get => {
                let _ = shared.sandbox.create_dir_all(dir);
            }
            Direction::Send => {
                lock(&shared.tree).ensure_dir(dir);
            }
        }
    }
}

#[async_trait]
impl Backend for MockBackend {
    fn peer(&self) -> PeerInfo {
        self.shared.peer.clone()
    }

    fn connection(&self) -> ConnectionState {
        self.shared.connection()
    }

    async fn list_dir(&self, path: RemotePath) -> Result<Vec<Entry>, BackendError> {
        let s = &self.shared;
        s.require_up()?;
        s.check_root(&path)?;
        sleep(s.model().rtt).await;
        s.require_up()?;
        lock(&s.tree).list(&path.components)
    }

    async fn start_transfer(&self, req: TransferRequest) -> Result<TransferId, BackendError> {
        let s = &self.shared;
        s.require_up()?;
        let mut plan = match &req {
            TransferRequest::Send { local, dest } => self.plan_send(local, dest)?,
            // The requested destination is ignored: the mock only writes to its sandbox.
            TransferRequest::Get { remote, dest: _ } => self.plan_get(remote)?,
        };
        plan.drop_at = lock(&s.drop_at).take();
        let id = TransferId(s.next_id.fetch_add(1, Ordering::SeqCst));
        let (cancel_tx, cancel_rx) = watch::channel(false);
        lock(&s.transfers).insert(id, cancel_tx);
        s.handle
            .spawn(Self::run(Arc::clone(&self.shared), id, plan, cancel_rx));
        Ok(id)
    }

    async fn cancel(&self, id: TransferId) -> Result<(), BackendError> {
        match lock(&self.shared.transfers).get(&id) {
            Some(tx) => {
                tx.send_replace(true);
                Ok(())
            }
            None => Err(BackendError::NotFound),
        }
    }

    fn events(&self) -> broadcast::Receiver<Event> {
        self.shared.events.subscribe()
    }
}

impl MockControl for MockBackend {
    fn set_link(&self, link: MockLink) {
        let s = &self.shared;
        s.link_generation.fetch_add(1, Ordering::SeqCst);
        s.set_link(match link {
            MockLink::Good => Link::Up(LinkQuality::Good),
            MockLink::Weak => Link::Up(LinkQuality::Weak),
            MockLink::Down => Link::Down,
        });
    }

    fn set_profile(&self, profile: Profile) {
        *lock(&self.shared.profile) = profile;
    }

    fn drop_connection(&self, kind: DropKind) {
        let s = &self.shared;
        let generation = s.link_generation.fetch_add(1, Ordering::SeqCst) + 1;
        match kind {
            DropKind::Permanent => s.set_link(Link::Down),
            DropKind::Blip => {
                let restore = match *s.link.borrow() {
                    Link::Up(q) => q,
                    _ => LinkQuality::Good,
                };
                s.set_link(Link::Reconnecting);
                let shared = Arc::clone(s);
                s.handle.spawn(async move {
                    sleep(BLIP_DURATION).await;
                    // Only restore if nothing else changed the link meanwhile.
                    if shared.link_generation.load(Ordering::SeqCst) == generation {
                        shared.set_link(Link::Up(restore));
                    }
                });
            }
        }
    }

    fn drop_at(&self, percent: u8) {
        *lock(&self.shared.drop_at) = Some(percent.min(100));
    }

    fn reset_sandbox(&self) -> Result<(), BackendError> {
        self.shared.sandbox.reset().map_err(|e| io_err(&e))
    }

    fn sandbox_root(&self) -> PathBuf {
        self.shared.sandbox.root_path().to_owned()
    }

    fn received_dir(&self) -> PathBuf {
        self.shared.sandbox.received_path()
    }
}
