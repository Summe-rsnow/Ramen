//! Named Pipe IPC client and background search/watch coordinator for `gozd` v0.7.1.

use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use goz_core::proto::{
    DaemonStatus, FrameDecoder, MAX_SERVER_FRAME, QueryRequest, QueryResults, Request, Response,
    ResultItem, VolumeGen, decode_response_frame, encode_request, feature, PROTO_VERSION,
};
use goz_core::types::SortSpec;
use parking_lot::{Condvar, Mutex};

use super::query::{FileCategory, compose_query};
use super::service::{DaemonServiceState, query_service_state};
use super::version::server_matches_pinned;

#[cfg(target_os = "windows")]
use goz_core::proto::PIPE_NAME;

const READ_DEADLINE: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const WATCH_POLL_INTERVAL: Duration = Duration::from_millis(25);
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(250);

/// Errors returned by [`GozClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    /// Daemon named pipe is absent or not running on this host.
    Unreachable,
    /// Pipe exists but its owner SID is not SYSTEM or Administrators (possible squatter).
    Untrusted,
    /// Daemon refused the request (`ProtoError` or incompatible protocol version).
    Refused(String),
    /// Frame decode error, timeout, unexpected response type, or pipe I/O failure.
    Protocol(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable => f.write_str("goz daemon is not running"),
            Self::Untrusted => f.write_str("pipe server failed trust check (not SYSTEM/Admin)"),
            Self::Refused(msg) | Self::Protocol(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for ClientError {}

/// Result of a [`GozClient::hello`] handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloInfo {
    pub proto: u16,
    pub server: String,
    pub ready: bool,
    pub features: Vec<String>,
}

impl HelloInfo {
    /// Returns `true` when the daemon advertised `feat` in `Hello.features`.
    pub fn has_feature(&self, feat: &str) -> bool {
        self.features.iter().any(|f| f == feat)
    }
}

/// Aggregated result from draining paginated `QueryResults` frames.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryOutcome {
    pub total: u64,
    pub items: Vec<ResultItem>,
    pub truncated: bool,
    pub volumes_incomplete: bool,
    pub metadata_pending: bool,
    pub total_size: Option<u64>,
}

/// Abstraction over a byte pipe so frame decoding and pagination are unit-testable on any OS.
pub(crate) trait PipeIo {
    fn available(&mut self) -> io::Result<u32>;
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()>;
    fn now(&self) -> Instant;
    fn pause(&mut self, time: Duration);
}

pub(crate) struct FramedPipe<P: PipeIo> {
    io: P,
    decoder: FrameDecoder,
    buf: Vec<u8>,
}

impl<P: PipeIo> FramedPipe<P> {
    pub(crate) fn new(io: P) -> Self {
        Self {
            io,
            decoder: FrameDecoder::new(MAX_SERVER_FRAME),
            buf: vec![0u8; 256 * 1024],
        }
    }

    pub(crate) fn send(&mut self, req: &Request) -> Result<(), ClientError> {
        let mut out = Vec::new();
        encode_request(req, &mut out);
        self.io
            .write_all(&out)
            .map_err(|e| ClientError::Protocol(format!("pipe write failed: {e}")))
    }

    pub(crate) fn next_response(
        &mut self,
        deadline: Option<Duration>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Response>, ClientError> {
        let mut expire_at = deadline.map(|d| self.io.now() + d);
        loop {
            if cancelled() {
                return Ok(None);
            }
            match self.decoder.next_frame() {
                Ok(Some(frame)) => {
                    let resp = decode_response_frame(&frame)
                        .map_err(|e| ClientError::Protocol(format!("malformed response: {e}")))?;
                    return Ok(Some(resp));
                }
                Ok(None) => {}
                Err(e) => return Err(ClientError::Protocol(format!("frame error: {e}"))),
            }

            let waiting = self
                .io
                .available()
                .map_err(|e| ClientError::Protocol(format!("pipe closed: {e}")))?;
            if waiting == 0 {
                if let Some(limit) = expire_at
                    && self.io.now() > limit
                {
                    return Err(ClientError::Protocol("daemon did not reply in time".to_owned()));
                }
                self.io.pause(if deadline.is_some() {
                    POLL_INTERVAL
                } else {
                    WATCH_POLL_INTERVAL
                });
                continue;
            }

            let n = self
                .io
                .read(&mut self.buf)
                .map_err(|e| ClientError::Protocol(format!("pipe read failed: {e}")))?;
            if n == 0 {
                return Err(ClientError::Protocol("daemon closed the pipe".to_owned()));
            }
            self.decoder.feed(&self.buf[..n]);
            expire_at = deadline.map(|d| self.io.now() + d);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn hello(&mut self) -> Result<HelloInfo, ClientError> {
        self.hello_cancellable(&|| false)
    }

    pub(crate) fn hello_cancellable(
        &mut self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<HelloInfo, ClientError> {
        if cancelled() {
            return Err(ClientError::Protocol("hello cancelled".to_owned()));
        }
        let req = Request::Hello {
            proto_min: PROTO_VERSION,
            proto_max: PROTO_VERSION,
            client: format!("ramen {}", env!("CARGO_PKG_VERSION")),
        };
        self.send(&req)?;
        match self.next_response(Some(READ_DEADLINE), cancelled)? {
            Some(Response::Hello {
                proto,
                server,
                ready,
                features,
            }) => {
                if proto != PROTO_VERSION {
                    return Err(ClientError::Refused(format!(
                        "unsupported goz protocol version {proto} (expected {PROTO_VERSION})"
                    )));
                }
                Ok(HelloInfo {
                    proto,
                    server,
                    ready,
                    features,
                })
            }
            Some(Response::Error { message, .. }) => Err(ClientError::Refused(message)),
            Some(_) => Err(ClientError::Protocol("unexpected reply to Hello".to_owned())),
            None => Err(ClientError::Protocol("hello cancelled".to_owned())),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn status(&mut self) -> Result<DaemonStatus, ClientError> {
        self.status_cancellable(&|| false)
    }

    pub(crate) fn status_cancellable(
        &mut self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DaemonStatus, ClientError> {
        if cancelled() {
            return Err(ClientError::Protocol("status cancelled".to_owned()));
        }
        self.send(&Request::Status)?;
        match self.next_response(Some(READ_DEADLINE), cancelled)? {
            Some(Response::Status(status)) => Ok(status),
            Some(Response::Error { message, .. }) => Err(ClientError::Refused(message)),
            Some(_) => Err(ClientError::Protocol("unexpected reply to Status".to_owned())),
            None => Err(ClientError::Protocol("status cancelled".to_owned())),
        }
    }

    pub(crate) fn query_cancellable(
        &mut self,
        mut req: QueryRequest,
        max_items: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<QueryOutcome>, ClientError> {
        if req.limit.is_none() {
            req.limit = Some(u32::try_from(max_items).unwrap_or(u32::MAX));
        }
        self.send(&Request::Query(req))?;

        let mut items = Vec::new();
        let mut total: u64;
        let mut volumes_incomplete = false;
        let mut metadata_pending = false;
        let mut total_size = None;
        let mut truncated = false;

        loop {
            let Some(resp) = self.next_response(Some(READ_DEADLINE), cancelled)? else {
                return Ok(None);
            };
            match resp {
                Response::Results(page) => {
                    let QueryResults {
                        total: page_total,
                        items: page_items,
                        more,
                        volumes_incomplete: page_incomplete,
                        metadata_pending: page_meta_pending,
                        total_size: page_total_size,
                        ..
                    } = page;

                    total = page_total;
                    volumes_incomplete |= page_incomplete;
                    metadata_pending |= page_meta_pending;
                    if total_size.is_none() {
                        total_size = page_total_size;
                    }

                    let remaining = max_items.saturating_sub(items.len());
                    if page_items.len() >= remaining {
                        items.extend(page_items.into_iter().take(remaining));
                        if more || (items.len() as u64) < page_total {
                            truncated = true;
                        }
                        break;
                    }
                    items.extend(page_items);
                    if !more {
                        truncated = (items.len() as u64) < page_total;
                        break;
                    }
                }
                Response::Error { message, .. } => return Err(ClientError::Refused(message)),
                _ => {
                    return Err(ClientError::Protocol(
                        "unexpected frame during Query response".to_owned(),
                    ));
                }
            }
        }

        Ok(Some(QueryOutcome {
            total,
            items,
            truncated,
            volumes_incomplete,
            metadata_pending,
            total_size,
        }))
    }
}

#[cfg(target_os = "windows")]
struct WinPipeIo(std::fs::File);

#[cfg(target_os = "windows")]
impl PipeIo for WinPipeIo {
    fn available(&mut self) -> io::Result<u32> {
        use std::os::windows::io::AsHandle;
        goz_winfs::pipe_bytes_available(self.0.as_handle()).map_err(io::Error::other)
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut self.0, buf)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        io::Write::write_all(&mut self.0, buf)
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn pause(&mut self, time: Duration) {
        thread::sleep(time);
    }
}

/// Blocking Windows Named Pipe client speaking the `goz` v0.7.1 framed protocol.
pub struct GozClient {
    #[cfg(target_os = "windows")]
    framed: FramedPipe<WinPipeIo>,
    #[cfg(not(target_os = "windows"))]
    _unsupported: (),
}

impl GozClient {
    /// Connects to `\\.\pipe\goz-v1` with `SECURITY_SQOS_PRESENT`, retries transient
    /// `ERROR_PIPE_BUSY`, and verifies server pipe ownership via `goz_winfs::pipe_server_is_trusted`.
    pub fn connect() -> Result<Self, ClientError> {
        Self::connect_cancellable(&|| false)
    }

    /// Connects to `\\.\pipe\goz-v1`, aborting early if `cancelled()` becomes true.
    pub fn connect_cancellable(cancelled: &dyn Fn() -> bool) -> Result<Self, ClientError> {
        #[cfg(target_os = "windows")]
        {
            use std::fs::OpenOptions;
            use std::os::windows::fs::OpenOptionsExt;
            use std::os::windows::io::AsHandle;

            const SECURITY_SQOS_PRESENT: u32 = 0x0010_0000;
            const ERROR_FILE_NOT_FOUND: i32 = 2;
            const ERROR_PATH_NOT_FOUND: i32 = 3;
            const ERROR_PIPE_BUSY: i32 = 231;

            let mut attempts = 0u32;
            let pipe = loop {
                if cancelled() {
                    return Err(ClientError::Protocol("connect cancelled".to_owned()));
                }
                match OpenOptions::new()
                    .read(true)
                    .write(true)
                    .custom_flags(SECURITY_SQOS_PRESENT)
                    .open(PIPE_NAME)
                {
                    Ok(file) => break file,
                    Err(e) => match e.raw_os_error() {
                        Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) => {
                            return Err(ClientError::Unreachable);
                        }
                        Some(ERROR_PIPE_BUSY) if attempts < 20 => {
                            attempts += 1;
                            if cancelled() {
                                return Err(ClientError::Protocol("connect cancelled".to_owned()));
                            }
                            thread::sleep(Duration::from_millis(25));
                        }
                        _ => {
                            return Err(ClientError::Protocol(format!(
                                "cannot open {PIPE_NAME}: {e}"
                            )));
                        }
                    },
                }
            };

            if cancelled() {
                return Err(ClientError::Protocol("connect cancelled".to_owned()));
            }

            match goz_winfs::pipe_server_is_trusted(pipe.as_handle()) {
                Ok(true) => {}
                Ok(false) | Err(_) => return Err(ClientError::Untrusted),
            }

            Ok(Self {
                framed: FramedPipe::new(WinPipeIo(pipe)),
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = cancelled;
            Err(ClientError::Unreachable)
        }
    }

    /// Performs the `Hello` protocol handshake and returns server capabilities.
    pub fn hello(&mut self) -> Result<HelloInfo, ClientError> {
        self.hello_cancellable(&|| false)
    }

    /// Performs the `Hello` protocol handshake, aborting early when `cancelled()` is true.
    pub fn hello_cancellable(
        &mut self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<HelloInfo, ClientError> {
        #[cfg(target_os = "windows")]
        {
            self.framed.hello_cancellable(cancelled)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = cancelled;
            Err(ClientError::Unreachable)
        }
    }

    /// Queries per-volume daemon index status.
    pub fn status(&mut self) -> Result<DaemonStatus, ClientError> {
        self.status_cancellable(&|| false)
    }

    /// Queries per-volume daemon index status, aborting early when `cancelled()` is true.
    pub fn status_cancellable(
        &mut self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DaemonStatus, ClientError> {
        #[cfg(target_os = "windows")]
        {
            self.framed.status_cancellable(cancelled)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = cancelled;
            Err(ClientError::Unreachable)
        }
    }

    /// Executes `req` and drains paginated binary `Results` frames up to `max_items`.
    pub fn query(
        &mut self,
        req: QueryRequest,
        max_items: usize,
    ) -> Result<QueryOutcome, ClientError> {
        self.query_cancellable(req, max_items, &|| false)?
            .ok_or_else(|| ClientError::Protocol("query cancelled".to_owned()))
    }

    /// Executes `req` and drains pages up to `max_items`, aborting early when `cancelled()` is true.
    pub fn query_cancellable(
        &mut self,
        req: QueryRequest,
        max_items: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<QueryOutcome>, ClientError> {
        #[cfg(target_os = "windows")]
        {
            self.framed.query_cancellable(req, max_items, cancelled)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (req, max_items, cancelled);
            Err(ClientError::Unreachable)
        }
    }

    /// Starts a dedicated Watch stream on this connection (`Request::Watch`).
    #[cfg(target_os = "windows")]
    fn start_watch(
        &mut self,
        min_interval_ms: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), ClientError> {
        if cancelled() {
            return Err(ClientError::Protocol("watch handshake cancelled".to_owned()));
        }
        self.framed.send(&Request::Watch {
            volumes: None,
            min_interval_ms,
        })?;
        match self.framed.next_response(Some(READ_DEADLINE), cancelled)? {
            Some(Response::Watching) => Ok(()),
            Some(Response::Error { message, .. }) => Err(ClientError::Refused(message)),
            Some(_) => Err(ClientError::Protocol("unexpected reply to Watch".to_owned())),
            None => Err(ClientError::Protocol("watch handshake cancelled".to_owned())),
        }
    }

    /// Waits for the next `Response::Changed` frame on an active Watch connection.
    #[cfg(target_os = "windows")]
    fn next_watch_change(&mut self, stop: &dyn Fn() -> bool) -> Result<Option<Vec<VolumeGen>>, ClientError> {
        match self.framed.next_response(None, stop)? {
            Some(Response::Changed { volumes }) => Ok(Some(volumes)),
            Some(Response::Error { message, .. }) => Err(ClientError::Refused(message)),
            Some(_) => Err(ClientError::Protocol("unexpected frame on watch pipe".to_owned())),
            None => Ok(None),
        }
    }
}

/// Parameters for a search submitted to [`SearchEngine`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchParams {
    pub raw_query: String,
    pub category: FileCategory,
    pub scope: Option<String>,
    pub sort: SortSpec,
    pub match_case: bool,
    pub max_items: usize,
    pub want_size: bool,
    pub want_mtime: bool,
}

impl Default for SearchParams {
    fn default() -> Self {
        Self {
            raw_query: String::new(),
            category: FileCategory::All,
            scope: None,
            sort: SortSpec::default(),
            match_case: false,
            max_items: 2000,
            want_size: true,
            want_mtime: true,
        }
    }
}

impl SearchParams {
    /// Builds the wire [`QueryRequest`] using [`compose_query`].
    pub fn to_query_request(&self) -> QueryRequest {
        QueryRequest {
            query: compose_query(&self.raw_query, self.category),
            scope: self.scope.clone(),
            sort: self.sort,
            offset: 0,
            limit: Some(u32::try_from(self.max_items).unwrap_or(u32::MAX)),
            want_size: self.want_size,
            want_mtime: self.want_mtime,
            match_case: self.match_case,
            want_total_size: self.want_size,
            ..Default::default()
        }
    }
}

/// Connection and readiness state of the `gozd` daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonConnectionState {
    Connected {
        server: String,
        ready: bool,
        version_matches: bool,
        supports_watch: bool,
        features: Vec<String>,
    },
    Offline(DaemonServiceState),
    Untrusted,
    Error(String),
}

/// Asynchronous events pushed by [`SearchEngine`] into its UI-pollable event queue.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    DaemonState(DaemonConnectionState),
    StatusUpdated(DaemonStatus),
    SearchResults {
        generation: u64,
        outcome: QueryOutcome,
    },
    SearchRefused {
        generation: u64,
        message: String,
    },
    SearchFailed {
        generation: u64,
        message: String,
    },
    IndexChanged {
        volumes: Vec<VolumeGen>,
    },
}

#[derive(Default)]
struct QueryInbox {
    pending_search: Option<(u64, SearchParams)>,
    last_search: Option<(u64, SearchParams)>,
    probe_requested: bool,
    status_requested: bool,
    refresh_requested: bool,
}

struct EngineShared {
    latest_gen: AtomicU64,
    shutdown: AtomicBool,
    auto_refresh_on_change: AtomicBool,
    watch_enabled: AtomicBool,
    query_done: AtomicBool,
    watch_done: AtomicBool,
    inbox: Mutex<QueryInbox>,
    query_cv: Condvar,
    watch_lock: Mutex<()>,
    watch_cv: Condvar,
    done_lock: Mutex<()>,
    done_cv: Condvar,
    events: Mutex<VecDeque<EngineEvent>>,
}

struct WorkerExitGuard<'a> {
    done: &'a AtomicBool,
    shared: &'a EngineShared,
}

impl Drop for WorkerExitGuard<'_> {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        let _guard = self.shared.done_lock.lock();
        self.shared.done_cv.notify_all();
    }
}

#[cfg(target_os = "windows")]
fn cancel_synchronous_io(handle: &JoinHandle<()>) {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    unsafe extern "system" {
        fn CancelSynchronousIo(h_thread: *mut c_void) -> i32;
    }

    // SAFETY: `handle.as_raw_handle()` is a live Win32 thread handle owned by `JoinHandle`
    // for the duration of this call; `CancelSynchronousIo` safely returns 0 if no I/O is pending.
    unsafe {
        let _ = CancelSynchronousIo(handle.as_raw_handle());
    }
}

impl EngineShared {
    fn push_event(&self, event: EngineEvent) {
        self.events.lock().push_back(event);
    }

    fn set_watch_enabled(&self, enabled: bool) {
        let prev = self.watch_enabled.swap(enabled, Ordering::AcqRel);
        if enabled && !prev {
            self.watch_cv.notify_all();
        }
    }

    fn request_probe(&self) {
        self.inbox.lock().probe_requested = true;
        self.query_cv.notify_all();
    }

    fn request_refresh_after_watch(&self) {
        let mut inbox = self.inbox.lock();
        inbox.status_requested = true;
        if self.auto_refresh_on_change.load(Ordering::Acquire) {
            inbox.refresh_requested = true;
        }
        self.query_cv.notify_all();
    }
}

/// Background coordinator managing search queries, daemon health probes, and live index watches.
pub struct SearchEngine {
    shared: Arc<EngineShared>,
    query_thread: Option<JoinHandle<()>>,
    watch_thread: Option<JoinHandle<()>>,
}

impl SearchEngine {
    /// Spawns the query worker and watch worker threads and queues an initial daemon probe.
    pub fn new() -> Self {
        let shared = Arc::new(EngineShared {
            latest_gen: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            auto_refresh_on_change: AtomicBool::new(true),
            watch_enabled: AtomicBool::new(false),
            query_done: AtomicBool::new(false),
            watch_done: AtomicBool::new(false),
            inbox: Mutex::new(QueryInbox {
                probe_requested: true,
                ..QueryInbox::default()
            }),
            query_cv: Condvar::new(),
            watch_lock: Mutex::new(()),
            watch_cv: Condvar::new(),
            done_lock: Mutex::new(()),
            done_cv: Condvar::new(),
            events: Mutex::new(VecDeque::new()),
        });

        let q_shared = Arc::clone(&shared);
        let query_thread = thread::Builder::new()
            .name("ramen-goz-query".to_owned())
            .spawn(move || query_worker_loop(&q_shared))
            .ok();

        let w_shared = Arc::clone(&shared);
        let watch_thread = thread::Builder::new()
            .name("ramen-goz-watch".to_owned())
            .spawn(move || watch_worker_loop(&w_shared))
            .ok();

        Self {
            shared,
            query_thread,
            watch_thread,
        }
    }

    /// Submits a new search, atomically cancelling any older in-flight search generation.
    /// Returns the assigned monotonic generation ID.
    pub fn submit_search(&self, params: SearchParams) -> u64 {
        let generation = self.shared.latest_gen.fetch_add(1, Ordering::AcqRel) + 1;
        {
            let mut inbox = self.shared.inbox.lock();
            inbox.pending_search = Some((generation, params));
        }
        self.shared.query_cv.notify_all();
        generation
    }

    /// Returns the most recently assigned search generation ID.
    pub fn current_generation(&self) -> u64 {
        self.shared.latest_gen.load(Ordering::Acquire)
    }

    /// Triggers an asynchronous daemon handshake + status probe.
    pub fn probe_daemon(&self) {
        self.shared.request_probe();
    }

    /// Triggers an asynchronous `Request::Status` refresh.
    pub fn request_status(&self) {
        self.shared.inbox.lock().status_requested = true;
        self.shared.query_cv.notify_all();
    }

    /// Re-runs the most recent search parameters without changing the generation ID.
    pub fn refresh_search(&self) {
        self.shared.inbox.lock().refresh_requested = true;
        self.shared.query_cv.notify_all();
    }

    /// Controls whether `IndexChanged` events automatically refresh the active search.
    pub fn set_auto_refresh(&self, enabled: bool) {
        self.shared
            .auto_refresh_on_change
            .store(enabled, Ordering::Release);
    }

    /// Drains all queued [`EngineEvent`]s in FIFO order for UI consumption.
    pub fn drain_events(&self) -> Vec<EngineEvent> {
        self.shared.events.lock().drain(..).collect()
    }

    /// Signals background workers to shut down, cancels pending synchronous pipe I/O on Windows,
    /// and waits up to [`SHUTDOWN_JOIN_TIMEOUT`] so app exit never hangs on a stalled daemon.
    pub fn shutdown(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.shared.watch_enabled.store(false, Ordering::Release);
        self.shared.latest_gen.fetch_add(1, Ordering::AcqRel);
        self.shared.query_cv.notify_all();
        self.shared.watch_cv.notify_all();

        #[cfg(target_os = "windows")]
        {
            if let Some(handle) = &self.query_thread {
                cancel_synchronous_io(handle);
            }
            if let Some(handle) = &self.watch_thread {
                cancel_synchronous_io(handle);
            }
        }

        if self.query_thread.is_some() || self.watch_thread.is_some() {
            let deadline = Instant::now() + SHUTDOWN_JOIN_TIMEOUT;
            let mut guard = self.shared.done_lock.lock();
            while (self.query_thread.is_some() && !self.shared.query_done.load(Ordering::Acquire))
                || (self.watch_thread.is_some()
                    && !self.shared.watch_done.load(Ordering::Acquire))
            {
                if Instant::now() >= deadline {
                    break;
                }
                let _ = self.shared.done_cv.wait_until(&mut guard, deadline);
            }
        }

        if let Some(handle) = self.query_thread.take() {
            if self.shared.query_done.load(Ordering::Acquire) {
                let _ = handle.join();
            } else {
                log::warn!("Detaching blocked goz query worker thread during shutdown");
            }
        }
        if let Some(handle) = self.watch_thread.take() {
            if self.shared.watch_done.load(Ordering::Acquire) {
                let _ = handle.join();
            } else {
                log::warn!("Detaching blocked goz watch worker thread during shutdown");
            }
        }
    }
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SearchEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

enum WorkItem {
    Probe,
    Status,
    Search { generation: u64, params: SearchParams },
}

fn next_work_item(shared: &EngineShared) -> Option<WorkItem> {
    let mut inbox = shared.inbox.lock();
    loop {
        if shared.shutdown.load(Ordering::Acquire) {
            return None;
        }
        if inbox.probe_requested {
            inbox.probe_requested = false;
            return Some(WorkItem::Probe);
        }
        if let Some((generation, params)) = inbox.pending_search.take() {
            inbox.last_search = Some((generation, params.clone()));
            inbox.refresh_requested = false;
            return Some(WorkItem::Search { generation, params });
        }
        if inbox.status_requested {
            inbox.status_requested = false;
            return Some(WorkItem::Status);
        }
        if inbox.refresh_requested {
            inbox.refresh_requested = false;
            if let Some((generation, params)) = inbox.last_search.clone() {
                return Some(WorkItem::Search { generation, params });
            }
        }
        shared.query_cv.wait(&mut inbox);
    }
}

fn query_worker_loop(shared: &EngineShared) {
    let _exit = WorkerExitGuard {
        done: &shared.query_done,
        shared,
    };
    while let Some(item) = next_work_item(shared) {
        match item {
            WorkItem::Probe => run_probe(shared),
            WorkItem::Status => run_status(shared),
            WorkItem::Search { generation, params } => run_search(shared, generation, params),
        }
    }
}

fn emit_connection_error(shared: &EngineShared, err: ClientError) {
    if shared.shutdown.load(Ordering::Acquire) {
        return;
    }
    shared.set_watch_enabled(false);
    match err {
        ClientError::Unreachable => {
            let svc = query_service_state().unwrap_or(DaemonServiceState::NotInstalled);
            shared.push_event(EngineEvent::DaemonState(DaemonConnectionState::Offline(svc)));
        }
        ClientError::Untrusted => {
            shared.push_event(EngineEvent::DaemonState(DaemonConnectionState::Untrusted));
        }
        ClientError::Refused(msg) | ClientError::Protocol(msg) => {
            shared.push_event(EngineEvent::DaemonState(DaemonConnectionState::Error(msg)));
        }
    }
}

fn run_probe(shared: &EngineShared) {
    let is_shutdown = || shared.shutdown.load(Ordering::Acquire);
    if is_shutdown() {
        return;
    }

    let mut client = match GozClient::connect_cancellable(&is_shutdown) {
        Ok(c) => c,
        Err(e) => {
            emit_connection_error(shared, e);
            return;
        }
    };

    let hello = match client.hello_cancellable(&is_shutdown) {
        Ok(h) => h,
        Err(e) => {
            emit_connection_error(shared, e);
            return;
        }
    };

    if is_shutdown() {
        return;
    }

    let supports_watch = hello.has_feature(feature::WATCH);
    let version_matches = server_matches_pinned(&hello.server);
    shared.push_event(EngineEvent::DaemonState(DaemonConnectionState::Connected {
        server: hello.server,
        ready: hello.ready,
        version_matches,
        supports_watch,
        features: hello.features,
    }));
    shared.set_watch_enabled(supports_watch);

    if let Ok(status) = client.status_cancellable(&is_shutdown) {
        if !is_shutdown() {
            shared.push_event(EngineEvent::StatusUpdated(status));
        }
    }
}

fn run_status(shared: &EngineShared) {
    let is_shutdown = || shared.shutdown.load(Ordering::Acquire);
    if is_shutdown() {
        return;
    }

    let mut client = match GozClient::connect_cancellable(&is_shutdown) {
        Ok(c) => c,
        Err(e) => {
            emit_connection_error(shared, e);
            return;
        }
    };
    match client.status_cancellable(&is_shutdown) {
        Ok(status) => {
            if !is_shutdown() {
                shared.push_event(EngineEvent::StatusUpdated(status));
            }
        }
        Err(e) => {
            emit_connection_error(shared, e);
        }
    }
}

fn run_search(shared: &EngineShared, generation: u64, params: SearchParams) {
    let is_stale = || {
        shared.shutdown.load(Ordering::Acquire)
            || shared.latest_gen.load(Ordering::Acquire) != generation
    };
    if is_stale() {
        return;
    }

    let mut client = match GozClient::connect_cancellable(&is_stale) {
        Ok(c) => c,
        Err(e) => {
            emit_connection_error(shared, e.clone());
            if !is_stale() {
                shared.push_event(EngineEvent::SearchFailed {
                    generation,
                    message: e.to_string(),
                });
            }
            return;
        }
    };

    let req = params.to_query_request();
    match client.query_cancellable(req, params.max_items, &is_stale) {
        Ok(Some(outcome)) => {
            if !is_stale() {
                shared.push_event(EngineEvent::SearchResults {
                    generation,
                    outcome,
                });
            }
        }
        Ok(None) => {}
        Err(ClientError::Refused(message)) => {
            if !is_stale() {
                shared.push_event(EngineEvent::SearchRefused {
                    generation,
                    message,
                });
            }
        }
        Err(e) => {
            if matches!(e, ClientError::Unreachable | ClientError::Untrusted) {
                emit_connection_error(shared, e.clone());
            }
            if !is_stale() {
                shared.push_event(EngineEvent::SearchFailed {
                    generation,
                    message: e.to_string(),
                });
            }
        }
    }
}

fn watch_worker_loop(shared: &EngineShared) {
    let _exit = WorkerExitGuard {
        done: &shared.watch_done,
        shared,
    };
    loop {
        {
            let mut guard = shared.watch_lock.lock();
            while !shared.shutdown.load(Ordering::Acquire)
                && !shared.watch_enabled.load(Ordering::Acquire)
            {
                shared.watch_cv.wait(&mut guard);
            }
        }

        if shared.shutdown.load(Ordering::Acquire) {
            return;
        }

        #[cfg(target_os = "windows")]
        {
            run_windows_watch_session(shared);
        }
        #[cfg(not(target_os = "windows"))]
        {
            shared.set_watch_enabled(false);
        }
    }
}

#[cfg(target_os = "windows")]
fn run_windows_watch_session(shared: &EngineShared) {
    let should_stop = || {
        shared.shutdown.load(Ordering::Acquire) || !shared.watch_enabled.load(Ordering::Acquire)
    };
    if should_stop() {
        return;
    }

    let mut client = match GozClient::connect_cancellable(&should_stop) {
        Ok(c) => c,
        Err(e) => {
            emit_connection_error(shared, e);
            return;
        }
    };

    let hello = match client.hello_cancellable(&should_stop) {
        Ok(h) => h,
        Err(e) => {
            emit_connection_error(shared, e);
            return;
        }
    };
    if !hello.has_feature(feature::WATCH) {
        shared.set_watch_enabled(false);
        return;
    }

    if let Err(e) = client.start_watch(250, &should_stop) {
        emit_connection_error(shared, e);
        return;
    }

    loop {
        if should_stop() {
            return;
        }
        match client.next_watch_change(&should_stop) {
            Ok(Some(volumes)) => {
                shared.push_event(EngineEvent::IndexChanged { volumes });
                shared.request_refresh_after_watch();
            }
            Ok(None) => return,
            Err(_) => {
                shared.set_watch_enabled(false);
                if !shared.shutdown.load(Ordering::Acquire) {
                    shared.request_probe();
                }
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goz_core::proto::{ProtoError, encode_response_frame};
    use goz_core::types::{SortDir, SortKey};

    struct ScriptedPipe {
        incoming: VecDeque<u8>,
        sent: Vec<u8>,
        elapsed: Instant,
    }

    impl ScriptedPipe {
        fn new(frames: Vec<u8>) -> Self {
            Self {
                incoming: frames.into(),
                sent: Vec::new(),
                elapsed: Instant::now(),
            }
        }
    }

    impl PipeIo for ScriptedPipe {
        fn available(&mut self) -> io::Result<u32> {
            Ok(u32::try_from(self.incoming.len()).unwrap_or(u32::MAX))
        }

        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(self.incoming.len());
            for slot in &mut buf[..n] {
                *slot = self.incoming.pop_front().unwrap();
            }
            Ok(n)
        }

        fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
            self.sent.extend_from_slice(buf);
            Ok(())
        }

        fn now(&self) -> Instant {
            self.elapsed
        }

        fn pause(&mut self, time: Duration) {
            self.elapsed += time;
        }
    }

    fn item(path: &str, is_dir: bool) -> ResultItem {
        ResultItem {
            path: path.to_owned(),
            size: if is_dir { None } else { Some(512) },
            mtime_ft: Some(133_500_000_000_000_000),
            is_dir,
            ..Default::default()
        }
    }

    #[test]
    fn decodes_hello_status_and_error_json_frames() {
        let mut wire = Vec::new();
        encode_response_frame(
            &Response::Hello {
                proto: PROTO_VERSION,
                server: "gozd 0.7.1".into(),
                ready: true,
                features: vec![feature::WATCH.into(), feature::SORT_EXTENSION.into()],
            },
            &mut wire,
        );
        encode_response_frame(
            &Response::Status(DaemonStatus {
                volumes: vec![],
                volumes_incomplete: false,
                process_working_set: 1024,
                process_private_bytes: 2048,
            }),
            &mut wire,
        );
        encode_response_frame(
            &Response::Error {
                code: ProtoError::BadQuery,
                message: "unbalanced quote".into(),
            },
            &mut wire,
        );

        let mut pipe = FramedPipe::new(ScriptedPipe::new(wire));
        let hello = pipe.hello().expect("hello succeeds");
        assert_eq!(hello.server, "gozd 0.7.1");
        assert!(hello.ready);
        assert!(hello.has_feature(feature::WATCH));

        let status = pipe.status().expect("status succeeds");
        assert_eq!(status.process_working_set, 1024);

        let err = pipe
            .query_cancellable(QueryRequest::default(), 100, &|| false)
            .unwrap_err();
        assert_eq!(err, ClientError::Refused("unbalanced quote".to_owned()));
    }

    #[test]
    fn decodes_multipage_binary_results_and_enforces_max_items_cap() {
        let mut wire = Vec::new();
        encode_response_frame(
            &Response::Results(QueryResults {
                total: 5,
                items: vec![item(r"C:\a.txt", false), item(r"C:\b.txt", false)],
                more: true,
                volumes_incomplete: false,
                metadata_pending: false,
                generation: 0,
                total_size: None,
            }),
            &mut wire,
        );
        encode_response_frame(
            &Response::Results(QueryResults {
                total: 5,
                items: vec![
                    item(r"C:\c", true),
                    item(r"C:\d.txt", false),
                    item(r"C:\e.txt", false),
                ],
                more: false,
                volumes_incomplete: true,
                metadata_pending: true,
                generation: 0,
                total_size: Some(2048),
            }),
            &mut wire,
        );

        // Full drain when max_items >= 5
        let mut full = FramedPipe::new(ScriptedPipe::new(wire.clone()));
        let outcome = full
            .query_cancellable(QueryRequest::default(), 100, &|| false)
            .unwrap()
            .unwrap();
        assert_eq!(outcome.total, 5);
        assert_eq!(outcome.items.len(), 5);
        assert!(!outcome.truncated);
        assert!(outcome.volumes_incomplete);
        assert!(outcome.metadata_pending);
        assert_eq!(outcome.items[2].path, r"C:\c");
        assert!(outcome.items[2].is_dir);

        // Capped at max_items = 3 across page boundary
        let mut capped = FramedPipe::new(ScriptedPipe::new(wire));
        let outcome_capped = capped
            .query_cancellable(QueryRequest::default(), 3, &|| false)
            .unwrap()
            .unwrap();
        assert_eq!(outcome_capped.total, 5);
        assert_eq!(outcome_capped.items.len(), 3);
        assert!(outcome_capped.truncated);
        assert_eq!(outcome_capped.items[2].path, r"C:\c");
    }

    #[test]
    fn search_params_builds_composed_query_request() {
        let params = SearchParams {
            raw_query: "lib.rs".into(),
            category: FileCategory::Code,
            scope: Some(r"C:\my\AI\nsh".into()),
            sort: SortSpec {
                key: SortKey::DateModified,
                dir: SortDir::Desc,
            },
            match_case: true,
            max_items: 500,
            want_size: true,
            want_mtime: true,
        };
        let req = params.to_query_request();
        assert!(req.query.starts_with("ext:"));
        assert!(req.query.ends_with(" lib.rs"));
        assert_eq!(req.scope.as_deref(), Some(r"C:\my\AI\nsh"));
        assert_eq!(req.limit, Some(500));
        assert!(req.match_case);
    }

    #[test]
    fn query_aborts_immediately_when_cancelled() {
        let mut wire = Vec::new();
        encode_response_frame(
            &Response::Results(QueryResults {
                total: 2,
                items: vec![item(r"C:\a.txt", false)],
                more: true,
                volumes_incomplete: false,
                metadata_pending: false,
                generation: 0,
                total_size: None,
            }),
            &mut wire,
        );
        let mut pipe = FramedPipe::new(ScriptedPipe::new(wire));
        let res = pipe
            .query_cancellable(QueryRequest::default(), 100, &|| true)
            .unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn search_engine_generation_coalesces_and_drains_events_fifo() {
        let shared = EngineShared {
            latest_gen: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            auto_refresh_on_change: AtomicBool::new(true),
            watch_enabled: AtomicBool::new(false),
            query_done: AtomicBool::new(false),
            watch_done: AtomicBool::new(false),
            inbox: Mutex::new(QueryInbox::default()),
            query_cv: Condvar::new(),
            watch_lock: Mutex::new(()),
            watch_cv: Condvar::new(),
            done_lock: Mutex::new(()),
            done_cv: Condvar::new(),
            events: Mutex::new(VecDeque::new()),
        };

        // Simulate rapid typing: gen 1 is overwritten by gen 2 before worker pops.
        let g1 = shared.latest_gen.fetch_add(1, Ordering::AcqRel) + 1;
        shared.inbox.lock().pending_search = Some((
            g1,
            SearchParams {
                raw_query: "r".into(),
                ..SearchParams::default()
            },
        ));
        let g2 = shared.latest_gen.fetch_add(1, Ordering::AcqRel) + 1;
        shared.inbox.lock().pending_search = Some((
            g2,
            SearchParams {
                raw_query: "ramen".into(),
                ..SearchParams::default()
            },
        ));
        assert_eq!(g1, 1);
        assert_eq!(g2, 2);

        let popped = shared.inbox.lock().pending_search.take().unwrap();
        assert_eq!(popped.0, 2);
        assert_eq!(popped.1.raw_query, "ramen");

        shared.push_event(EngineEvent::DaemonState(DaemonConnectionState::Untrusted));
        shared.push_event(EngineEvent::SearchRefused {
            generation: 2,
            message: "bad syntax".into(),
        });
        let drained: Vec<_> = shared.events.lock().drain(..).collect();
        assert_eq!(drained.len(), 2);
        assert_eq!(
            drained[0],
            EngineEvent::DaemonState(DaemonConnectionState::Untrusted)
        );
        assert_eq!(
            drained[1],
            EngineEvent::SearchRefused {
                generation: 2,
                message: "bad syntax".into(),
            }
        );
        assert!(shared.events.lock().is_empty());
    }

    #[test]
    fn hello_and_status_abort_immediately_when_cancelled() {
        let mut pipe = FramedPipe::new(ScriptedPipe::new(Vec::new()));
        let hello_err = pipe.hello_cancellable(&|| true).unwrap_err();
        assert_eq!(hello_err, ClientError::Protocol("hello cancelled".to_owned()));

        let status_err = pipe.status_cancellable(&|| true).unwrap_err();
        assert_eq!(
            status_err,
            ClientError::Protocol("status cancelled".to_owned())
        );
    }

    #[test]
    fn search_engine_shutdown_completes_promptly_even_when_worker_is_blocked() {
        let shared = Arc::new(EngineShared {
            latest_gen: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            auto_refresh_on_change: AtomicBool::new(true),
            watch_enabled: AtomicBool::new(false),
            query_done: AtomicBool::new(false),
            watch_done: AtomicBool::new(false),
            inbox: Mutex::new(QueryInbox::default()),
            query_cv: Condvar::new(),
            watch_lock: Mutex::new(()),
            watch_cv: Condvar::new(),
            done_lock: Mutex::new(()),
            done_cv: Condvar::new(),
            events: Mutex::new(VecDeque::new()),
        });

        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let gate_worker = Arc::clone(&gate);
        let shared_worker = Arc::clone(&shared);
        let blocked_query = thread::spawn(move || {
            let _exit = WorkerExitGuard {
                done: &shared_worker.query_done,
                shared: &shared_worker,
            };
            let (lock, cv) = &*gate_worker;
            let mut released = lock.lock();
            while !*released {
                cv.wait(&mut released);
            }
        });

        let mut engine = SearchEngine {
            shared,
            query_thread: Some(blocked_query),
            watch_thread: None,
        };

        let start = Instant::now();
        engine.shutdown();
        let elapsed = start.elapsed();

        // Release the synthetic blocked thread so the test process exits cleanly.
        {
            let (lock, cv) = &*gate;
            *lock.lock() = true;
            cv.notify_all();
        }

        assert!(
            elapsed < Duration::from_secs(1),
            "shutdown should detach blocked worker within timeout, took {elapsed:?}"
        );
    }

    #[test]
    fn search_engine_normal_shutdown_joins_promptly() {
        let mut engine = SearchEngine::new();
        let start = Instant::now();
        engine.shutdown();
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(1),
            "normal SearchEngine::shutdown took too long: {elapsed:?}"
        );
    }
}
