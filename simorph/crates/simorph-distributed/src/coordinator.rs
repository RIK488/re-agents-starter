//! The coordinator: queue, placement, failover, exactly-once results.

use crate::auth;
use crate::inventory::ResourceInventory;
use crate::protocol::*;
use crate::wire;
use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_plugin_sdk::protocol::Response;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::io::BufReader;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Coordinator configuration.
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Shared secret; `None` disables authentication (local development only).
    pub secret: Option<Vec<u8>>,
    /// A worker silent for longer than this is declared dead.
    pub heartbeat_timeout: Duration,
    /// Maximum number of queued (not yet running) tasks; `submit` fails beyond.
    pub max_queue: usize,
    /// A queued task that no registered worker could ever run is declared
    /// unschedulable after this delay (workers may still be joining).
    pub unschedulable_grace: Duration,
    /// Default per-attempt timeout.
    pub default_timeout: Duration,
    /// Default maximum number of attempts (infrastructure failures only).
    pub max_attempts: u32,
    /// Dispatcher tick.
    pub tick: Duration,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            secret: None,
            heartbeat_timeout: Duration::from_secs(3),
            max_queue: 100_000,
            unschedulable_grace: Duration::from_secs(5),
            default_timeout: Duration::from_secs(600),
            max_attempts: 3,
            tick: Duration::from_millis(10),
        }
    }
}

/// A task submitted to the cluster.
#[derive(Debug, Clone)]
pub struct TaskSpec {
    /// Component name as registered in the workers' `ComponentSet`.
    pub component: String,
    pub request: TaskRequest,
    /// Higher runs first.
    pub priority: i32,
    pub requirements: ResourceRequirements,
    pub timeout: Option<Duration>,
    pub max_attempts: Option<u32>,
}

impl TaskSpec {
    pub fn new(component: &str, request: TaskRequest) -> Self {
        Self {
            component: component.into(),
            request,
            priority: 0,
            requirements: ResourceRequirements::default(),
            timeout: None,
            max_attempts: None,
        }
    }
    pub fn priority(mut self, p: i32) -> Self {
        self.priority = p;
        self
    }
    pub fn requirements(mut self, r: ResourceRequirements) -> Self {
        self.requirements = r;
        self
    }
    pub fn timeout(mut self, t: Duration) -> Self {
        self.timeout = Some(t);
        self
    }
    pub fn max_attempts(mut self, n: u32) -> Self {
        self.max_attempts = Some(n);
        self
    }
    fn dedup_key(&self) -> String {
        format!("{}:{}", self.component, self.request.content_hash())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
    Unschedulable,
}

/// Public view of a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutcome {
    pub task_id: TaskId,
    pub kind: OutcomeKind,
    pub result: Option<TaskResult>,
    pub error: Option<String>,
    pub attempts: u32,
    /// Workers that ran an attempt, in order.
    pub workers: Vec<String>,
    /// Global order in which terminal results were recorded (0-based).
    pub completion_index: Option<u64>,
}

impl TaskOutcome {
    pub fn is_terminal(&self) -> bool {
        !matches!(self.kind, OutcomeKind::Queued | OutcomeKind::Running)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerMetrics {
    pub id: String,
    pub alive: bool,
    pub slots: usize,
    pub running: usize,
    pub completed: u64,
    pub busy_s: f64,
    /// busy time / (slots × time since registration).
    pub utilisation: f64,
    pub gpus: u32,
    pub cpu_cores: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorMetrics {
    pub elapsed_s: f64,
    pub submitted: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub unschedulable: u64,
    pub retries: u64,
    pub timeouts: u64,
    /// Late results for tasks already completed / reassigned (dropped).
    pub duplicates_ignored: u64,
    /// Submissions answered by an existing task with the same request hash.
    pub dedup_hits: u64,
    pub queued: usize,
    pub running: usize,
    pub throughput_tasks_per_s: f64,
    pub workers: Vec<WorkerMetrics>,
    pub rejected_workers: Vec<String>,
}

enum TaskState {
    Queued,
    Running { worker: String, started: Instant },
    Done(TaskResult),
    Failed(String),
    Cancelled,
    Unschedulable(String),
}

struct TaskEntry {
    spec: TaskSpec,
    state: TaskState,
    attempts: u32,
    submitted: Instant,
    workers: Vec<String>,
    finished: Option<u64>,
}

struct WorkerSlot {
    inventory: ResourceInventory,
    components: HashSet<String>,
    slots: usize,
    writer: Arc<Mutex<TcpStream>>,
    running: HashSet<TaskId>,
    last_seen: Instant,
    registered: Instant,
    alive: bool,
    completed: u64,
    busy_s: f64,
}

#[derive(Default)]
struct Counters {
    submitted: u64,
    completed: u64,
    failed: u64,
    cancelled: u64,
    unschedulable: u64,
    retries: u64,
    timeouts: u64,
    duplicates_ignored: u64,
    dedup_hits: u64,
    finished_seq: u64,
}

struct State {
    workers: HashMap<String, WorkerSlot>,
    queue: BinaryHeap<(i32, Reverse<u64>, TaskId)>,
    tasks: HashMap<TaskId, TaskEntry>,
    dedup: HashMap<String, TaskId>,
    seq: u64,
    c: Counters,
    rejected: Vec<String>,
}

struct Inner {
    cfg: CoordinatorConfig,
    state: Mutex<State>,
    changed: Condvar,
    shutdown: AtomicBool,
    started: Instant,
}

/// The coordinator.  Dropping it shuts the cluster down.
pub struct Coordinator {
    inner: Arc<Inner>,
    addr: SocketAddr,
}

type Outbox = Vec<(Arc<Mutex<TcpStream>>, CoordinatorMsg, String)>;

impl Coordinator {
    /// Bind and start accepting workers.
    pub fn bind(addr: &str, cfg: CoordinatorConfig) -> Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let local = listener.local_addr()?;
        let inner = Arc::new(Inner {
            cfg,
            state: Mutex::new(State {
                workers: HashMap::new(),
                queue: BinaryHeap::new(),
                tasks: HashMap::new(),
                dedup: HashMap::new(),
                seq: 0,
                c: Counters::default(),
                rejected: vec![],
            }),
            changed: Condvar::new(),
            shutdown: AtomicBool::new(false),
            started: Instant::now(),
        });
        let acc = inner.clone();
        std::thread::Builder::new()
            .name("simorph-accept".into())
            .spawn(move || accept_loop(acc, listener))?;
        let disp = inner.clone();
        std::thread::Builder::new()
            .name("simorph-dispatch".into())
            .spawn(move || dispatch_loop(disp))?;
        Ok(Self { inner, addr: local })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Number of live workers.
    pub fn live_workers(&self) -> usize {
        self.inner
            .state
            .lock()
            .workers
            .values()
            .filter(|w| w.alive)
            .count()
    }

    /// Block until `n` workers are alive or the timeout elapses.
    pub fn wait_for_workers(&self, n: usize, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let mut st = self.inner.state.lock();
        loop {
            if st.workers.values().filter(|w| w.alive).count() >= n {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(SimorphError::Timeout(timeout.as_millis() as u64));
            }
            self.inner
                .changed
                .wait_for(&mut st, Duration::from_millis(20));
        }
    }

    /// Submit a task.  Identical requests (same component and request hash)
    /// return the existing task id unless that task failed or was cancelled.
    /// Fails with back-pressure when the queue is full.
    pub fn submit(&self, spec: TaskSpec) -> Result<TaskId> {
        let mut st = self.inner.state.lock();
        let key = spec.dedup_key();
        if let Some(existing) = st.dedup.get(&key).copied() {
            if let Some(t) = st.tasks.get(&existing) {
                if !matches!(
                    t.state,
                    TaskState::Failed(_) | TaskState::Cancelled | TaskState::Unschedulable(_)
                ) {
                    st.c.dedup_hits += 1;
                    return Ok(existing);
                }
            }
        }
        let queued = st
            .tasks
            .values()
            .filter(|t| matches!(t.state, TaskState::Queued))
            .count();
        if queued >= self.inner.cfg.max_queue {
            return Err(SimorphError::Other(format!(
                "back-pressure: queue full ({queued} queued, limit {})",
                self.inner.cfg.max_queue
            )));
        }
        let id = TaskId::new();
        st.seq += 1;
        let seq = st.seq;
        st.queue.push((spec.priority, Reverse(seq), id));
        st.dedup.insert(key, id);
        st.tasks.insert(
            id,
            TaskEntry {
                spec,
                state: TaskState::Queued,
                attempts: 0,
                submitted: Instant::now(),
                workers: vec![],
                finished: None,
            },
        );
        st.c.submitted += 1;
        drop(st);
        self.inner.changed.notify_all();
        Ok(id)
    }

    /// Submit, waiting for queue space up to `timeout`.
    pub fn submit_blocking(&self, spec: TaskSpec, timeout: Duration) -> Result<TaskId> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.submit(spec.clone()) {
                Err(SimorphError::Other(m))
                    if m.starts_with("back-pressure") && Instant::now() < deadline =>
                {
                    let mut st = self.inner.state.lock();
                    self.inner
                        .changed
                        .wait_for(&mut st, Duration::from_millis(10));
                }
                r => return r,
            }
        }
    }

    pub fn status(&self, id: TaskId) -> Option<TaskOutcome> {
        let st = self.inner.state.lock();
        st.tasks.get(&id).map(|t| outcome(id, t))
    }

    /// Wait for a task to reach a terminal state.
    pub fn wait(&self, id: TaskId, timeout: Duration) -> Result<TaskOutcome> {
        let deadline = Instant::now() + timeout;
        let mut st = self.inner.state.lock();
        loop {
            let t = st
                .tasks
                .get(&id)
                .ok_or_else(|| SimorphError::NotFound(format!("task {id}")))?;
            let o = outcome(id, t);
            if o.is_terminal() {
                return Ok(o);
            }
            if Instant::now() >= deadline {
                return Err(SimorphError::Timeout(timeout.as_millis() as u64));
            }
            self.inner
                .changed
                .wait_for(&mut st, Duration::from_millis(20));
        }
    }

    pub fn wait_all(&self, ids: &[TaskId], timeout: Duration) -> Result<Vec<TaskOutcome>> {
        let deadline = Instant::now() + timeout;
        ids.iter()
            .map(|id| self.wait(*id, deadline.saturating_duration_since(Instant::now())))
            .collect()
    }

    /// Cancel a task: queued tasks are dropped, running ones are cancelled on
    /// their worker; a late result is ignored.
    pub fn cancel(&self, id: TaskId) -> Result<()> {
        let mut out: Outbox = vec![];
        {
            let mut st = self.inner.state.lock();
            let state = &mut *st;
            let t = state
                .tasks
                .get_mut(&id)
                .ok_or_else(|| SimorphError::NotFound(format!("task {id}")))?;
            match &t.state {
                TaskState::Queued => {
                    t.state = TaskState::Cancelled;
                    state.c.cancelled += 1;
                }
                TaskState::Running { worker, .. } => {
                    let w = worker.clone();
                    t.state = TaskState::Cancelled;
                    state.c.cancelled += 1;
                    if let Some(ws) = state.workers.get_mut(&w) {
                        ws.running.remove(&id);
                        out.push((ws.writer.clone(), CoordinatorMsg::Cancel { task_id: id }, w));
                    }
                }
                _ => {}
            }
        }
        flush(&self.inner, out);
        self.inner.changed.notify_all();
        Ok(())
    }

    pub fn metrics(&self) -> CoordinatorMetrics {
        let st = self.inner.state.lock();
        let elapsed = self.inner.started.elapsed().as_secs_f64();
        let workers = st
            .workers
            .iter()
            .map(|(id, w)| {
                let span = w.registered.elapsed().as_secs_f64().max(1e-9);
                WorkerMetrics {
                    id: id.clone(),
                    alive: w.alive,
                    slots: w.slots,
                    running: w.running.len(),
                    completed: w.completed,
                    busy_s: w.busy_s,
                    utilisation: w.busy_s / (span * w.slots.max(1) as f64),
                    gpus: w.inventory.gpu_count(),
                    cpu_cores: w.inventory.cpu_cores,
                }
            })
            .collect();
        CoordinatorMetrics {
            elapsed_s: elapsed,
            submitted: st.c.submitted,
            completed: st.c.completed,
            failed: st.c.failed,
            cancelled: st.c.cancelled,
            unschedulable: st.c.unschedulable,
            retries: st.c.retries,
            timeouts: st.c.timeouts,
            duplicates_ignored: st.c.duplicates_ignored,
            dedup_hits: st.c.dedup_hits,
            queued: st
                .tasks
                .values()
                .filter(|t| matches!(t.state, TaskState::Queued))
                .count(),
            running: st
                .tasks
                .values()
                .filter(|t| matches!(t.state, TaskState::Running { .. }))
                .count(),
            throughput_tasks_per_s: st.c.completed as f64 / elapsed.max(1e-9),
            workers,
            rejected_workers: st.rejected.clone(),
        }
    }

    /// Ask every worker to stop and stop accepting connections.
    pub fn shutdown(&self) {
        if self.inner.shutdown.swap(true, Ordering::SeqCst) {
            return;
        }
        let writers: Vec<_> = self
            .inner
            .state
            .lock()
            .workers
            .iter()
            .filter(|(_, w)| w.alive)
            .map(|(id, w)| (w.writer.clone(), CoordinatorMsg::Shutdown, id.clone()))
            .collect();
        flush(&self.inner, writers);
        let _ = TcpStream::connect(self.addr); // unblock accept()
        self.inner.changed.notify_all();
    }
}

impl Drop for Coordinator {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn outcome(id: TaskId, t: &TaskEntry) -> TaskOutcome {
    let (kind, result, error) = match &t.state {
        TaskState::Queued => (OutcomeKind::Queued, None, None),
        TaskState::Running { .. } => (OutcomeKind::Running, None, None),
        TaskState::Done(r) => (OutcomeKind::Done, Some(r.clone()), None),
        TaskState::Failed(e) => (OutcomeKind::Failed, None, Some(e.clone())),
        TaskState::Cancelled => (OutcomeKind::Cancelled, None, Some("cancelled".into())),
        TaskState::Unschedulable(e) => (OutcomeKind::Unschedulable, None, Some(e.clone())),
    };
    TaskOutcome {
        task_id: id,
        kind,
        result,
        error,
        attempts: t.attempts,
        workers: t.workers.clone(),
        completion_index: t.finished,
    }
}

/// Send messages outside the state lock; a failed write marks the worker dead.
fn flush(inner: &Arc<Inner>, out: Outbox) {
    for (w, msg, worker) in out {
        let ok = wire::send(&mut w.lock(), &msg).is_ok();
        if !ok {
            let mut st = inner.state.lock();
            declare_dead(&mut st, &inner.cfg, &worker, "write failed");
        }
    }
}

fn accept_loop(inner: Arc<Inner>, listener: TcpListener) {
    for conn in listener.incoming() {
        if inner.shutdown.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = conn else { continue };
        let i2 = inner.clone();
        let _ = std::thread::Builder::new()
            .name("simorph-conn".into())
            .spawn(move || handle_connection(i2, stream));
    }
}

fn handle_connection(inner: Arc<Inner>, stream: TcpStream) {
    let _ = stream.set_nodelay(true);
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_half);
    let mut writer = stream;
    let nonce = auth::nonce();
    if wire::send(
        &mut writer,
        &CoordinatorMsg::Challenge {
            nonce: nonce.clone(),
            protocol: DISTRIBUTED_PROTOCOL_VERSION.into(),
        },
    )
    .is_err()
    {
        return;
    }
    let _ = writer.set_read_timeout(Some(Duration::from_secs(10)));
    let hello: Option<WorkerMsg> = wire::recv(&mut reader).ok().flatten();
    let _ = writer.set_read_timeout(None);
    let Some(WorkerMsg::Hello {
        worker_id,
        protocol,
        inventory,
        components,
        slots,
        auth: response,
    }) = hello
    else {
        let _ = wire::send(
            &mut writer,
            &CoordinatorMsg::Rejected {
                reason: "expected hello".into(),
            },
        );
        return;
    };
    let reject = |reason: String, writer: &mut TcpStream| {
        // Record before replying so observers never see a rejected worker exit
        // before the rejection is visible in the metrics.
        inner
            .state
            .lock()
            .rejected
            .push(format!("{worker_id}: {reason}"));
        let _ = wire::send(
            writer,
            &CoordinatorMsg::Rejected {
                reason: reason.clone(),
            },
        );
        let _ = writer.shutdown(Shutdown::Both);
    };
    if protocol != DISTRIBUTED_PROTOCOL_VERSION {
        return reject(format!("protocol `{protocol}` unsupported"), &mut writer);
    }
    if let Some(secret) = &inner.cfg.secret {
        let ok = response
            .as_deref()
            .map(|r| auth::verify(secret, &nonce, &worker_id, r))
            .unwrap_or(false);
        if !ok {
            return reject("authentication failed".into(), &mut writer);
        }
    }
    {
        let st = inner.state.lock();
        if st.workers.get(&worker_id).map(|w| w.alive).unwrap_or(false) {
            drop(st);
            return reject("worker id already connected".into(), &mut writer);
        }
    }
    if wire::send(
        &mut writer,
        &CoordinatorMsg::Welcome {
            worker_id: worker_id.clone(),
        },
    )
    .is_err()
    {
        return;
    }
    // A stalled worker must never block the dispatcher indefinitely.
    let _ = writer.set_write_timeout(Some(Duration::from_secs(5)));
    let shared = Arc::new(Mutex::new(writer));
    {
        let mut st = inner.state.lock();
        let now = Instant::now();
        st.workers.insert(
            worker_id.clone(),
            WorkerSlot {
                inventory,
                components: components.into_iter().collect(),
                slots: slots.max(1),
                writer: shared.clone(),
                running: HashSet::new(),
                last_seen: now,
                registered: now,
                alive: true,
                completed: 0,
                busy_s: 0.0,
            },
        );
    }
    inner.changed.notify_all();
    tracing::info!(worker = %worker_id, "worker registered");
    loop {
        match wire::recv::<WorkerMsg>(&mut reader) {
            Ok(Some(msg)) => {
                let mut st = inner.state.lock();
                if !st.workers.get(&worker_id).map(|w| w.alive).unwrap_or(false) {
                    // declared dead (heartbeat loss) while we were blocked: drop late
                    // traffic but count late results so exactly-once is observable.
                    if let WorkerMsg::Done { .. } = msg {
                        st.c.duplicates_ignored += 1;
                    }
                    continue;
                }
                if let Some(w) = st.workers.get_mut(&worker_id) {
                    w.last_seen = Instant::now();
                }
                match msg {
                    WorkerMsg::Heartbeat { .. } | WorkerMsg::Hello { .. } => {}
                    WorkerMsg::Done { task_id, response } => {
                        on_done(&mut st, &inner.cfg, &worker_id, task_id, response)
                    }
                }
                drop(st);
                inner.changed.notify_all();
            }
            Ok(None) | Err(_) => {
                let mut st = inner.state.lock();
                declare_dead(&mut st, &inner.cfg, &worker_id, "connection closed");
                drop(st);
                inner.changed.notify_all();
                break;
            }
        }
    }
}

fn on_done(st: &mut State, cfg: &CoordinatorConfig, worker: &str, id: TaskId, response: Response) {
    if let Some(w) = st.workers.get_mut(worker) {
        w.running.remove(&id);
    }
    let Some(t) = st.tasks.get_mut(&id) else {
        st.c.duplicates_ignored += 1;
        return;
    };
    let started = match &t.state {
        TaskState::Running { worker: w, started } if w == worker => *started,
        _ => {
            st.c.duplicates_ignored += 1;
            return;
        }
    };
    let busy = started.elapsed().as_secs_f64();
    if let Response::Error { message, .. } = &response {
        if message.starts_with(crate::worker::TIMEOUT_PREFIX) {
            st.c.timeouts += 1;
            if let Some(w) = st.workers.get_mut(worker) {
                w.busy_s += busy;
            }
            requeue_or_fail(st, cfg, id, &format!("timeout on {worker}"));
            return;
        }
    }
    let t = st.tasks.get_mut(&id).expect("checked above");
    t.finished = Some(st.c.finished_seq);
    st.c.finished_seq += 1;
    match response {
        Response::Result { result } => {
            t.state = TaskState::Done(result);
            st.c.completed += 1;
        }
        Response::Error { message, .. } => {
            // Component errors are deterministic: not retried.
            t.state = TaskState::Failed(format!("component error on {worker}: {message}"));
            st.c.failed += 1;
        }
        other => {
            t.state = TaskState::Failed(format!("unexpected response from {worker}: {other:?}"));
            st.c.failed += 1;
        }
    }
    if let Some(w) = st.workers.get_mut(worker) {
        w.completed += 1;
        w.busy_s += busy;
    }
}

/// Mark a worker dead and requeue (or fail) its running tasks.
fn declare_dead(st: &mut State, cfg: &CoordinatorConfig, worker: &str, why: &str) {
    let Some(w) = st.workers.get_mut(worker) else {
        return;
    };
    if !w.alive {
        return;
    }
    w.alive = false;
    let _ = w.writer.lock().shutdown(Shutdown::Both);
    let running: Vec<TaskId> = w.running.drain().collect();
    tracing::warn!(worker, why, tasks = running.len(), "worker lost");
    for id in running {
        requeue_or_fail(st, cfg, id, &format!("worker {worker} lost ({why})"));
    }
}

fn requeue_or_fail(st: &mut State, cfg: &CoordinatorConfig, id: TaskId, why: &str) {
    let Some(t) = st.tasks.get_mut(&id) else {
        return;
    };
    if !matches!(t.state, TaskState::Running { .. }) {
        return;
    }
    let max = t.spec.max_attempts.unwrap_or(cfg.max_attempts);
    if t.attempts < max {
        t.state = TaskState::Queued;
        let prio = t.spec.priority;
        st.seq += 1;
        let seq = st.seq;
        st.queue.push((prio, Reverse(seq), id));
        st.c.retries += 1;
    } else {
        t.state = TaskState::Failed(format!("{why}; {} attempt(s) exhausted", t.attempts));
        t.finished = Some(st.c.finished_seq);
        st.c.finished_seq += 1;
        st.c.failed += 1;
    }
}

fn capable(w: &WorkerSlot, spec: &TaskSpec) -> bool {
    let r = &spec.requirements;
    w.alive
        && w.components.contains(&spec.component)
        && w.inventory.gpu_count() >= r.gpu_count
        && (r.memory_mb == 0 || w.inventory.memory_mb >= r.memory_mb)
        && (w.inventory.cpu_cores as f64) >= r.cpu_cores
        && (r.gpu_memory_mb == 0
            || w.inventory
                .gpus
                .iter()
                .filter(|g| g.memory_mb >= r.gpu_memory_mb)
                .count() as u32
                >= r.gpu_count.max(1))
}

fn dispatch_loop(inner: Arc<Inner>) {
    while !inner.shutdown.load(Ordering::SeqCst) {
        let mut out: Outbox = vec![];
        {
            let mut st = inner.state.lock();
            let now = Instant::now();
            // 1. heartbeat loss
            let silent: Vec<String> = st
                .workers
                .iter()
                .filter(|(_, w)| {
                    w.alive && now.duration_since(w.last_seen) > inner.cfg.heartbeat_timeout
                })
                .map(|(id, _)| id.clone())
                .collect();
            for w in silent {
                declare_dead(&mut st, &inner.cfg, &w, "heartbeat timeout");
            }
            // 2. per-attempt timeouts
            let expired: Vec<(TaskId, String)> = st
                .tasks
                .iter()
                .filter_map(|(id, t)| match &t.state {
                    TaskState::Running { worker, started }
                        if now.duration_since(*started)
                            > t.spec.timeout.unwrap_or(inner.cfg.default_timeout) =>
                    {
                        Some((*id, worker.clone()))
                    }
                    _ => None,
                })
                .collect();
            for (id, worker) in expired {
                st.c.timeouts += 1;
                if let Some(w) = st.workers.get_mut(&worker) {
                    w.running.remove(&id);
                    out.push((
                        w.writer.clone(),
                        CoordinatorMsg::Cancel { task_id: id },
                        worker.clone(),
                    ));
                }
                requeue_or_fail(&mut st, &inner.cfg, id, &format!("timeout on {worker}"));
            }
            // 3. placement
            let mut deferred = vec![];
            while let Some((prio, seq, id)) = st.queue.pop() {
                let Some(t) = st.tasks.get(&id) else { continue };
                if !matches!(t.state, TaskState::Queued) {
                    continue; // cancelled while queued
                }
                let spec = t.spec.clone();
                let submitted = t.submitted;
                let state = &mut *st;
                let mut candidates: Vec<(&String, &mut WorkerSlot)> = state
                    .workers
                    .iter_mut()
                    .filter(|(_, w)| capable(w, &spec))
                    .collect();
                if candidates.is_empty() {
                    if now.duration_since(submitted) > inner.cfg.unschedulable_grace {
                        let inv: Vec<String> = state
                            .workers
                            .iter()
                            .filter(|(_, w)| w.alive)
                            .map(|(id, w)| {
                                format!(
                                    "{id}: {} cores, {} MB, {} GPU, components {}",
                                    w.inventory.cpu_cores,
                                    w.inventory.memory_mb,
                                    w.inventory.gpu_count(),
                                    w.components.contains(&spec.component)
                                )
                            })
                            .collect();
                        let reason = format!(
                            "no registered worker satisfies component `{}` with {} GPU(s), {} MB memory, {} cores; workers: [{}]",
                            spec.component,
                            spec.requirements.gpu_count,
                            spec.requirements.memory_mb,
                            spec.requirements.cpu_cores,
                            inv.join("; ")
                        );
                        if let Some(t) = state.tasks.get_mut(&id) {
                            t.state = TaskState::Unschedulable(reason);
                        }
                        state.c.unschedulable += 1;
                    } else {
                        deferred.push((prio, seq, id));
                    }
                    continue;
                }
                candidates.retain(|(_, w)| w.running.len() < w.slots);
                candidates.sort_by(|a, b| {
                    let la = a.1.running.len() as f64 / a.1.slots as f64;
                    let lb = b.1.running.len() as f64 / b.1.slots as f64;
                    la.partial_cmp(&lb)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.0.cmp(b.0))
                });
                let Some((wid, w)) = candidates.into_iter().next() else {
                    deferred.push((prio, seq, id));
                    continue;
                };
                let wid = wid.clone();
                w.running.insert(id);
                let writer = w.writer.clone();
                let t = state.tasks.get_mut(&id).expect("exists");
                t.attempts += 1;
                t.workers.push(wid.clone());
                t.state = TaskState::Running {
                    worker: wid.clone(),
                    started: now,
                };
                let timeout_ms = spec
                    .timeout
                    .unwrap_or(inner.cfg.default_timeout)
                    .as_millis() as u64;
                out.push((
                    writer,
                    CoordinatorMsg::Assign {
                        task_id: id,
                        component: spec.component.clone(),
                        request: spec.request.clone(),
                        timeout_ms,
                    },
                    wid,
                ));
            }
            for d in deferred {
                st.queue.push(d);
            }
        }
        let progressed = !out.is_empty();
        flush(&inner, out);
        if progressed {
            inner.changed.notify_all();
        }
        let mut st = inner.state.lock();
        inner.changed.wait_for(&mut st, inner.cfg.tick);
    }
}
