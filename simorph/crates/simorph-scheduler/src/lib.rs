//! # simorph-scheduler
//!
//! A resource-aware scheduler for directed acyclic graphs of tasks.
//!
//! * Dependencies, priorities and a [`ResourcePool`] decide admission.
//! * Each task runs as a Tokio future with its own timeout and
//!   [`CancellationToken`]; cancelling a task cancels its dependents.
//! * Failures are isolated: only dependents of a failed task are skipped.
//! * Retries follow a [`RetryPolicy`] with exponential backoff.
//! * [`Scheduler::snapshot`] serialises the state of every task;
//!   [`Scheduler::resume`] continues an interrupted run, re-executing only
//!   tasks that had not finished.
//! * [`SchedulerEvent`]s are broadcast for supervision and metrics.

use indexmap::IndexMap;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use simorph_core::{
    CancellationToken, Deadline, ResourceRequirements, Result, SimorphError, TaskId,
};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinSet;

/// Retry policy with exponential backoff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Total attempts including the first (1 = no retry).
    pub max_attempts: u32,
    pub initial_backoff_ms: u64,
    pub multiplier: f64,
    pub max_backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff_ms: 100,
            multiplier: 2.0,
            max_backoff_ms: 10_000,
        }
    }
}

impl RetryPolicy {
    pub fn attempts(n: u32) -> Self {
        Self {
            max_attempts: n.max(1),
            ..Default::default()
        }
    }
    pub fn backoff_for(&self, attempt: u32) -> Duration {
        let ms =
            self.initial_backoff_ms as f64 * self.multiplier.powi(attempt.saturating_sub(1) as i32);
        Duration::from_millis(ms.min(self.max_backoff_ms as f64) as u64)
    }
}

/// Static description of a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: TaskId,
    pub name: String,
    #[serde(default)]
    pub deps: Vec<TaskId>,
    #[serde(default)]
    pub resources: ResourceRequirements,
    /// Higher runs first among ready tasks.
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub retry: RetryPolicy,
}

impl TaskSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            name: name.into(),
            deps: vec![],
            resources: ResourceRequirements {
                cpu_cores: 1.0,
                ..Default::default()
            },
            priority: 0,
            timeout_ms: None,
            retry: RetryPolicy::default(),
        }
    }
    pub fn depends_on(mut self, id: TaskId) -> Self {
        self.deps.push(id);
        self
    }
    pub fn priority(mut self, p: i32) -> Self {
        self.priority = p;
        self
    }
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout_ms = Some(d.as_millis() as u64);
        self
    }
    pub fn retry(mut self, r: RetryPolicy) -> Self {
        self.retry = r;
        self
    }
    pub fn resources(mut self, r: ResourceRequirements) -> Self {
        self.resources = r;
        self
    }
    pub fn cpu(mut self, cores: f64) -> Self {
        self.resources.cpu_cores = cores;
        self
    }
    pub fn gpu(mut self, n: u32) -> Self {
        self.resources.gpu_count = n;
        self
    }
}

/// Lifecycle state of a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Running {
        attempt: u32,
    },
    /// Waiting for backoff before the next attempt.
    Retrying {
        attempt: u32,
        error: String,
    },
    Succeeded {
        output: serde_json::Value,
    },
    Failed {
        error: String,
        attempts: u32,
    },
    TimedOut {
        attempts: u32,
    },
    Cancelled,
    /// Not run because a dependency failed, timed out or was cancelled.
    Skipped {
        reason: String,
    },
}

impl TaskState {
    pub fn is_terminal(&self) -> bool {
        !matches!(
            self,
            TaskState::Pending | TaskState::Running { .. } | TaskState::Retrying { .. }
        )
    }
    pub fn succeeded(&self) -> bool {
        matches!(self, TaskState::Succeeded { .. })
    }
}

/// Capacity available to the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourcePool {
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub gpus: u32,
}

impl ResourcePool {
    pub fn local() -> Self {
        Self {
            cpu_cores: std::thread::available_parallelism()
                .map(|n| n.get() as f64)
                .unwrap_or(1.0),
            memory_mb: 8192,
            gpus: 0,
        }
    }
    fn fits(&self, r: &ResourceRequirements) -> bool {
        r.cpu_cores <= self.cpu_cores + 1e-9
            && r.memory_mb <= self.memory_mb
            && r.gpu_count <= self.gpus
    }
    fn take(&mut self, r: &ResourceRequirements) {
        self.cpu_cores -= r.cpu_cores;
        self.memory_mb -= r.memory_mb;
        self.gpus -= r.gpu_count;
    }
    fn give(&mut self, r: &ResourceRequirements) {
        self.cpu_cores += r.cpu_cores;
        self.memory_mb += r.memory_mb;
        self.gpus += r.gpu_count;
    }
}

/// Context handed to a task body.
#[derive(Debug, Clone)]
pub struct TaskCtx {
    pub id: TaskId,
    pub name: String,
    pub attempt: u32,
    pub cancel: CancellationToken,
    pub deadline: Option<Deadline>,
    /// Outputs of direct dependencies.
    pub inputs: HashMap<TaskId, serde_json::Value>,
}

impl TaskCtx {
    pub fn check(&self) -> Result<()> {
        self.cancel.check()?;
        if let Some(d) = self.deadline {
            if d.expired() {
                return Err(SimorphError::Timeout(0));
            }
        }
        Ok(())
    }
}

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
pub type TaskFn = Arc<dyn Fn(TaskCtx) -> BoxFuture<Result<serde_json::Value>> + Send + Sync>;

/// Build a `TaskFn` from an async closure.
pub fn task_fn<F, Fut>(f: F) -> TaskFn
where
    F: Fn(TaskCtx) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<serde_json::Value>> + Send + 'static,
{
    Arc::new(move |ctx| Box::pin(f(ctx)))
}

/// Events emitted during a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchedulerEvent {
    pub task: TaskId,
    pub name: String,
    pub state: TaskState,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Serialisable checkpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchedulerSnapshot {
    pub pool: ResourcePool,
    pub tasks: Vec<TaskRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub spec: TaskSpec,
    pub state: TaskState,
    pub attempts: u32,
}

struct Entry {
    spec: TaskSpec,
    body: TaskFn,
    state: TaskState,
    attempts: u32,
    cancel: CancellationToken,
}

/// Handle to request cancellation of tasks while a run is in progress.
#[derive(Clone, Default)]
pub struct CancelHandle {
    requested: Arc<Mutex<HashSet<TaskId>>>,
    tokens: Arc<Mutex<HashMap<TaskId, CancellationToken>>>,
}

impl CancelHandle {
    pub fn cancel(&self, id: TaskId) {
        self.requested.lock().insert(id);
        if let Some(t) = self.tokens.lock().get(&id) {
            t.cancel();
        }
    }
    pub fn cancel_all(&self) {
        let ids: Vec<TaskId> = self.tokens.lock().keys().copied().collect();
        for id in ids {
            self.cancel(id);
        }
        self.requested.lock().insert(TaskId::deterministic("*"));
    }
}

/// The scheduler.
pub struct Scheduler {
    total_pool: ResourcePool,
    tasks: IndexMap<TaskId, Entry>,
    events: broadcast::Sender<SchedulerEvent>,
    cancel_handle: CancelHandle,
}

impl Scheduler {
    pub fn new(pool: ResourcePool) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            total_pool: pool,
            tasks: IndexMap::new(),
            events,
            cancel_handle: CancelHandle::default(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SchedulerEvent> {
        self.events.subscribe()
    }

    pub fn cancel_handle(&self) -> CancelHandle {
        self.cancel_handle.clone()
    }

    /// Add a task.  Dependencies must already be registered; resources must
    /// fit the pool.
    pub fn add(&mut self, spec: TaskSpec, body: TaskFn) -> Result<TaskId> {
        if self.tasks.contains_key(&spec.id) {
            return Err(SimorphError::InvalidInput(format!(
                "task {} already added",
                spec.id
            )));
        }
        for d in &spec.deps {
            if !self.tasks.contains_key(d) {
                return Err(SimorphError::InvalidInput(format!(
                    "task `{}` depends on unknown task {d}",
                    spec.name
                )));
            }
        }
        if !self.total_pool.fits(&spec.resources) {
            return Err(SimorphError::InvalidInput(format!(
                "task `{}` needs {:?} but the pool only has {:?}",
                spec.name, spec.resources, self.total_pool
            )));
        }
        let id = spec.id;
        let cancel = CancellationToken::new();
        self.cancel_handle.tokens.lock().insert(id, cancel.clone());
        self.tasks.insert(
            id,
            Entry {
                spec,
                body,
                state: TaskState::Pending,
                attempts: 0,
                cancel,
            },
        );
        Ok(id)
    }

    pub fn state(&self, id: TaskId) -> Option<&TaskState> {
        self.tasks.get(&id).map(|e| &e.state)
    }

    pub fn states(&self) -> IndexMap<TaskId, TaskState> {
        self.tasks
            .iter()
            .map(|(k, v)| (*k, v.state.clone()))
            .collect()
    }

    pub fn snapshot(&self) -> SchedulerSnapshot {
        SchedulerSnapshot {
            pool: self.total_pool,
            tasks: self
                .tasks
                .values()
                .map(|e| TaskRecord {
                    spec: e.spec.clone(),
                    // a task that was running when the snapshot was taken is re-run
                    state: match &e.state {
                        TaskState::Running { .. } | TaskState::Retrying { .. } => {
                            TaskState::Pending
                        }
                        s => s.clone(),
                    },
                    attempts: e.attempts,
                })
                .collect(),
        }
    }

    /// Rebuild a scheduler from a snapshot.  `bodies` supplies the function
    /// for every task not yet finished (finished tasks keep their output
    /// and are not re-executed).
    pub fn resume(snapshot: SchedulerSnapshot, bodies: HashMap<TaskId, TaskFn>) -> Result<Self> {
        let mut s = Scheduler::new(snapshot.pool);
        for rec in snapshot.tasks {
            let id = rec.spec.id;
            let body = match bodies.get(&id) {
                Some(b) => b.clone(),
                None if rec.state.is_terminal() => {
                    task_fn(|_| async { Ok(serde_json::Value::Null) })
                }
                None => {
                    return Err(SimorphError::InvalidInput(format!(
                        "no body supplied for unfinished task {id}"
                    )))
                }
            };
            s.add(rec.spec, body)?;
            let e = s.tasks.get_mut(&id).unwrap();
            e.state = rec.state;
            e.attempts = rec.attempts;
        }
        Ok(s)
    }

    fn emit(&self, id: TaskId) {
        if let Some(e) = self.tasks.get(&id) {
            let _ = self.events.send(SchedulerEvent {
                task: id,
                name: e.spec.name.clone(),
                state: e.state.clone(),
                timestamp: chrono::Utc::now(),
            });
        }
    }

    fn set_state(&mut self, id: TaskId, state: TaskState) {
        if let Some(e) = self.tasks.get_mut(&id) {
            e.state = state;
        }
        self.emit(id);
    }

    /// Mark every (transitive) dependent of `id` that is still pending as skipped.
    fn skip_dependents(&mut self, id: TaskId, reason: &str) {
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let dependents: Vec<TaskId> = self
                .tasks
                .iter()
                .filter(|(_, e)| {
                    e.spec.deps.contains(&cur)
                        && matches!(e.state, TaskState::Pending | TaskState::Retrying { .. })
                })
                .map(|(k, _)| *k)
                .collect();
            for d in dependents {
                self.set_state(
                    d,
                    TaskState::Skipped {
                        reason: reason.to_string(),
                    },
                );
                stack.push(d);
            }
        }
    }

    fn ready_tasks(&self) -> Vec<TaskId> {
        let mut ready: Vec<(i32, usize, TaskId)> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, (_, e))| matches!(e.state, TaskState::Pending))
            .filter(|(_, (_, e))| {
                e.spec.deps.iter().all(|d| {
                    self.tasks
                        .get(d)
                        .map(|x| x.state.succeeded())
                        .unwrap_or(false)
                })
            })
            .map(|(i, (id, e))| (e.spec.priority, i, *id))
            .collect();
        // highest priority first, then insertion order: deterministic
        ready.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        ready.into_iter().map(|(_, _, id)| id).collect()
    }

    fn apply_cancel_requests(&mut self) {
        let requested: Vec<TaskId> = self
            .cancel_handle
            .requested
            .lock()
            .iter()
            .copied()
            .collect();
        let all = requested.contains(&TaskId::deterministic("*"));
        let ids: Vec<TaskId> = self.tasks.keys().copied().collect();
        for id in ids {
            let e = &self.tasks[&id];
            if (all || requested.contains(&id))
                && matches!(e.state, TaskState::Pending | TaskState::Retrying { .. })
            {
                self.set_state(id, TaskState::Cancelled);
                self.skip_dependents(id, "dependency cancelled");
            }
        }
    }

    /// Run until every task is terminal.  Returns the final states.
    pub async fn run(&mut self) -> IndexMap<TaskId, TaskState> {
        let mut pool = self.total_pool;
        let mut running: JoinSet<(
            TaskId,
            u32,
            std::result::Result<Result<serde_json::Value>, tokio::time::error::Elapsed>,
        )> = JoinSet::new();
        let mut backoffs: JoinSet<TaskId> = JoinSet::new();
        let mut in_flight: HashMap<TaskId, ResourceRequirements> = HashMap::new();
        // Account for tasks resumed in Running state (none: snapshot maps them to Pending).
        loop {
            self.apply_cancel_requests();
            // Admission.
            for id in self.ready_tasks() {
                let e = &self.tasks[&id];
                if !pool.fits(&e.spec.resources) {
                    continue;
                }
                pool.take(&e.spec.resources);
                in_flight.insert(id, e.spec.resources.clone());
                let attempt = e.attempts + 1;
                let body = e.body.clone();
                let timeout = e.spec.timeout_ms.map(Duration::from_millis);
                let ctx = TaskCtx {
                    id,
                    name: e.spec.name.clone(),
                    attempt,
                    cancel: e.cancel.clone(),
                    deadline: timeout.map(Deadline::after),
                    inputs: e
                        .spec
                        .deps
                        .iter()
                        .filter_map(|d| match &self.tasks.get(d)?.state {
                            TaskState::Succeeded { output } => Some((*d, output.clone())),
                            _ => None,
                        })
                        .collect(),
                };
                self.tasks.get_mut(&id).unwrap().attempts = attempt;
                self.set_state(id, TaskState::Running { attempt });
                let fut = body(ctx);
                running.spawn(async move {
                    let r = match timeout {
                        Some(t) => tokio::time::timeout(t, fut).await,
                        None => Ok(fut.await),
                    };
                    (id, attempt, r)
                });
            }
            if running.is_empty() && backoffs.is_empty() {
                // Nothing running: anything still pending can never start
                // (unsatisfiable deps) — mark it skipped and finish.
                let stuck: Vec<TaskId> = self
                    .tasks
                    .iter()
                    .filter(|(_, e)| matches!(e.state, TaskState::Pending))
                    .map(|(k, _)| *k)
                    .collect();
                if stuck.is_empty() {
                    break;
                }
                for id in stuck {
                    self.set_state(
                        id,
                        TaskState::Skipped {
                            reason: "dependency did not succeed".into(),
                        },
                    );
                }
                break;
            }
            tokio::select! {
                Some(joined) = running.join_next(), if !running.is_empty() => {
                    let (id, attempt, outcome) = match joined {
                        Ok(x) => x,
                        Err(e) => {
                            tracing::error!("task join error: {e}");
                            continue;
                        }
                    };
                    if let Some(r) = in_flight.remove(&id) {
                        pool.give(&r);
                    }
                    let policy = self.tasks[&id].spec.retry.clone();
                    let was_cancelled = self.tasks[&id].cancel.is_cancelled();
                    match outcome {
                        Ok(Ok(v)) => self.set_state(id, TaskState::Succeeded { output: v }),
                        _ if was_cancelled => {
                            self.set_state(id, TaskState::Cancelled);
                            self.skip_dependents(id, "dependency cancelled");
                        }
                        Ok(Err(e)) => {
                            if attempt < policy.max_attempts {
                                let delay = policy.backoff_for(attempt);
                                self.set_state(id, TaskState::Retrying { attempt, error: e.to_string() });
                                backoffs.spawn(async move { tokio::time::sleep(delay).await; id });
                            } else {
                                self.set_state(id, TaskState::Failed { error: e.to_string(), attempts: attempt });
                                self.skip_dependents(id, "dependency failed");
                            }
                        }
                        Err(_elapsed) => {
                            self.tasks[&id].cancel.cancel();
                            if attempt < policy.max_attempts {
                                // fresh token for the retry
                                let tok = CancellationToken::new();
                                self.tasks.get_mut(&id).unwrap().cancel = tok.clone();
                                self.cancel_handle.tokens.lock().insert(id, tok);
                                let delay = policy.backoff_for(attempt);
                                self.set_state(id, TaskState::Retrying { attempt, error: "timeout".into() });
                                backoffs.spawn(async move { tokio::time::sleep(delay).await; id });
                            } else {
                                self.set_state(id, TaskState::TimedOut { attempts: attempt });
                                self.skip_dependents(id, "dependency timed out");
                            }
                        }
                    }
                }
                Some(Ok(id)) = backoffs.join_next(), if !backoffs.is_empty() => {
                    if matches!(self.tasks[&id].state, TaskState::Retrying { .. }) {
                        self.set_state(id, TaskState::Pending);
                    }
                }
            }
        }
        self.states()
    }

    pub fn spec(&self, id: TaskId) -> Option<&TaskSpec> {
        self.tasks.get(&id).map(|e| &e.spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    fn quick(v: i64) -> TaskFn {
        task_fn(move |_| async move { Ok(serde_json::json!(v)) })
    }

    #[tokio::test]
    async fn dependency_order_and_input_passing() {
        let mut s = Scheduler::new(ResourcePool {
            cpu_cores: 4.0,
            memory_mb: 1024,
            gpus: 0,
        });
        let order = Arc::new(Mutex::new(vec![]));
        let mk = |name: &'static str, order: Arc<Mutex<Vec<&'static str>>>| {
            task_fn(move |ctx: TaskCtx| {
                let order = order.clone();
                async move {
                    order.lock().push(name);
                    let sum: i64 = ctx.inputs.values().filter_map(|v| v.as_i64()).sum();
                    Ok(serde_json::json!(sum + 1))
                }
            })
        };
        let a = s.add(TaskSpec::new("a"), mk("a", order.clone())).unwrap();
        let b = s
            .add(TaskSpec::new("b").depends_on(a), mk("b", order.clone()))
            .unwrap();
        let c = s
            .add(TaskSpec::new("c").depends_on(a), mk("c", order.clone()))
            .unwrap();
        let d = s
            .add(
                TaskSpec::new("d").depends_on(b).depends_on(c),
                mk("d", order.clone()),
            )
            .unwrap();
        let states = s.run().await;
        let o = order.lock().clone();
        assert_eq!(o[0], "a");
        assert_eq!(o[3], "d");
        assert!(
            matches!(states[&d], TaskState::Succeeded { ref output } if output == &serde_json::json!(5))
        );
        assert!(states.values().all(|s| s.succeeded()));
    }

    #[tokio::test]
    async fn resource_admission_limits_concurrency() {
        let mut s = Scheduler::new(ResourcePool {
            cpu_cores: 2.0,
            memory_mb: 1024,
            gpus: 0,
        });
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        for i in 0..8 {
            let (cur, pk) = (current.clone(), peak.clone());
            s.add(
                TaskSpec::new(format!("t{i}")).cpu(1.0),
                task_fn(move |_| {
                    let (cur, pk) = (cur.clone(), pk.clone());
                    async move {
                        let n = cur.fetch_add(1, Ordering::SeqCst) + 1;
                        pk.fetch_max(n, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        cur.fetch_sub(1, Ordering::SeqCst);
                        Ok(serde_json::Value::Null)
                    }
                }),
            )
            .unwrap();
        }
        let states = s.run().await;
        assert!(states.values().all(|s| s.succeeded()));
        assert_eq!(peak.load(Ordering::SeqCst), 2);
        // oversubscribed task is rejected at submission
        assert!(s.add(TaskSpec::new("big").cpu(3.0), quick(0)).is_err());
        assert!(s.add(TaskSpec::new("gpu").gpu(1), quick(0)).is_err());
    }

    #[tokio::test]
    async fn timeout_fires_and_dependents_skip() {
        let mut s = Scheduler::new(ResourcePool {
            cpu_cores: 2.0,
            memory_mb: 1024,
            gpus: 0,
        });
        let slow = s
            .add(
                TaskSpec::new("slow").timeout(Duration::from_millis(50)),
                task_fn(|_| async {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    Ok(serde_json::Value::Null)
                }),
            )
            .unwrap();
        let dep = s
            .add(TaskSpec::new("dep").depends_on(slow), quick(1))
            .unwrap();
        let other = s.add(TaskSpec::new("other"), quick(2)).unwrap();
        let start = std::time::Instant::now();
        let states = s.run().await;
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(matches!(states[&slow], TaskState::TimedOut { attempts: 1 }));
        assert!(matches!(states[&dep], TaskState::Skipped { .. }));
        assert!(states[&other].succeeded(), "failure isolation");
    }

    #[tokio::test]
    async fn retry_succeeds_on_second_attempt() {
        let mut s = Scheduler::new(ResourcePool {
            cpu_cores: 1.0,
            memory_mb: 1024,
            gpus: 0,
        });
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let id = s
            .add(
                TaskSpec::new("flaky").retry(RetryPolicy {
                    max_attempts: 3,
                    initial_backoff_ms: 10,
                    ..Default::default()
                }),
                task_fn(move |ctx: TaskCtx| {
                    let c = c.clone();
                    async move {
                        c.fetch_add(1, Ordering::SeqCst);
                        if ctx.attempt < 2 {
                            Err(SimorphError::Other("transient".into()))
                        } else {
                            Ok(serde_json::json!(ctx.attempt))
                        }
                    }
                }),
            )
            .unwrap();
        let mut rx = s.subscribe();
        let states = s.run().await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(
            matches!(states[&id], TaskState::Succeeded { ref output } if output == &serde_json::json!(2))
        );
        let mut saw_retrying = false;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev.state, TaskState::Retrying { attempt: 1, .. }) {
                saw_retrying = true;
            }
        }
        assert!(saw_retrying);
    }

    #[tokio::test]
    async fn cancellation_propagates_to_dependents() {
        let mut s = Scheduler::new(ResourcePool {
            cpu_cores: 2.0,
            memory_mb: 1024,
            gpus: 0,
        });
        let long = s
            .add(
                TaskSpec::new("long"),
                task_fn(|ctx: TaskCtx| async move {
                    loop {
                        ctx.check()?;
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                }),
            )
            .unwrap();
        let child = s
            .add(TaskSpec::new("child").depends_on(long), quick(1))
            .unwrap();
        let grandchild = s
            .add(TaskSpec::new("grandchild").depends_on(child), quick(1))
            .unwrap();
        let independent = s.add(TaskSpec::new("independent"), quick(1)).unwrap();
        let handle = s.cancel_handle();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            handle.cancel(long);
        });
        let states = s.run().await;
        assert_eq!(states[&long], TaskState::Cancelled);
        assert!(matches!(states[&child], TaskState::Skipped { .. }));
        assert!(matches!(states[&grandchild], TaskState::Skipped { .. }));
        assert!(states[&independent].succeeded());
    }

    #[tokio::test]
    async fn snapshot_and_resume_complete_remaining_tasks() {
        let mut s = Scheduler::new(ResourcePool {
            cpu_cores: 1.0,
            memory_mb: 1024,
            gpus: 0,
        });
        let a = s.add(TaskSpec::new("a"), quick(10)).unwrap();
        let b_spec = TaskSpec::new("b").depends_on(a);
        let b = b_spec.id;
        s.add(
            b_spec,
            task_fn(|ctx: TaskCtx| async move {
                let a_out = ctx
                    .inputs
                    .values()
                    .next()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                Ok(serde_json::json!(a_out + 5))
            }),
        )
        .unwrap();
        // Simulate an interruption after `a` finished: run `a` alone by
        // cancelling `b`, then resume from the snapshot with `b` pending.
        s.cancel_handle().cancel(b);
        let states = s.run().await;
        assert!(states[&a].succeeded());
        let mut snap = s.snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        snap = serde_json::from_str(&json).unwrap();
        // Treat the cancelled task as pending for the resume.
        for t in &mut snap.tasks {
            if t.spec.id == b {
                t.state = TaskState::Pending;
            }
        }
        let a_calls = Arc::new(AtomicU32::new(0));
        let ac = a_calls.clone();
        let mut bodies: HashMap<TaskId, TaskFn> = HashMap::new();
        bodies.insert(
            a,
            task_fn(move |_| {
                let ac = ac.clone();
                async move {
                    ac.fetch_add(1, Ordering::SeqCst);
                    Ok(serde_json::json!(-1))
                }
            }),
        );
        bodies.insert(
            b,
            task_fn(|ctx: TaskCtx| async move {
                let a_out = ctx
                    .inputs
                    .values()
                    .next()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                Ok(serde_json::json!(a_out + 5))
            }),
        );
        let mut resumed = Scheduler::resume(snap, bodies).unwrap();
        let states = resumed.run().await;
        assert_eq!(
            a_calls.load(Ordering::SeqCst),
            0,
            "finished task is not re-run"
        );
        assert!(
            matches!(states[&b], TaskState::Succeeded { ref output } if output == &serde_json::json!(15))
        );
    }

    #[tokio::test]
    async fn priority_ordering_is_deterministic() {
        let mut s = Scheduler::new(ResourcePool {
            cpu_cores: 1.0,
            memory_mb: 1024,
            gpus: 0,
        });
        let order = Arc::new(Mutex::new(vec![]));
        for (name, prio) in [("low", 0), ("high", 10), ("mid", 5), ("high2", 10)] {
            let o = order.clone();
            s.add(
                TaskSpec::new(name).priority(prio),
                task_fn(move |ctx: TaskCtx| {
                    let o = o.clone();
                    async move {
                        o.lock().push(ctx.name.clone());
                        Ok(serde_json::Value::Null)
                    }
                }),
            )
            .unwrap();
        }
        s.run().await;
        assert_eq!(order.lock().clone(), vec!["high", "high2", "mid", "low"]);
    }
}
