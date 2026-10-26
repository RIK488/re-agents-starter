//! # simorph-orchestrator
//!
//! Executes a [`RunGraph`]: a DAG of component invocations where outputs
//! of one node are wired into inputs of another.  Each node becomes a
//! scheduler task; the executor runs the component with its timeout; node
//! events are broadcast for live supervision; metrics are recorded in a
//! [`Metrics`] registry.
//!
//! [`Supervisor`] wraps a component factory and re-instantiates the
//! component (e.g. a crashed subprocess) up to a restart budget.

use indexmap::IndexMap;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use simorph_core::{
    CancellationToken, Component, ComponentManifest, ExecutionControl, Result, SimorphError,
    TaskId, TaskRequest, TaskResult,
};
use simorph_execution::Executor;
use simorph_plugin_sdk::{ComponentSet, SharedComponent};
use simorph_scheduler::{
    task_fn, ResourcePool, RetryPolicy, Scheduler, SchedulerEvent, TaskSpec, TaskState,
};
use simorph_telemetry::{Labels, Metrics};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// A data dependency: `from_node.from_output` → this node's `to_input`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    pub from_node: String,
    pub from_output: String,
    pub to_input: String,
}

/// One invocation in the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunNode {
    /// Component name in the [`ComponentSet`].
    pub component: String,
    /// Static inputs (wired inputs are added at run time).
    #[serde(default)]
    pub inputs: IndexMap<String, serde_json::Value>,
    #[serde(default)]
    pub wires: Vec<Wire>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default = "one")]
    pub fidelity: f64,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub retry: RetryPolicy,
    #[serde(default)]
    pub priority: i32,
}

fn one() -> f64 {
    1.0
}

impl RunNode {
    pub fn new(component: &str) -> Self {
        Self {
            component: component.into(),
            inputs: IndexMap::new(),
            wires: vec![],
            seed: None,
            fidelity: 1.0,
            config: serde_json::Value::Null,
            timeout_ms: None,
            retry: RetryPolicy::default(),
            priority: 0,
        }
    }
    pub fn input(mut self, name: &str, v: impl Into<serde_json::Value>) -> Self {
        self.inputs.insert(name.into(), v.into());
        self
    }
    pub fn wire(mut self, from_node: &str, from_output: &str, to_input: &str) -> Self {
        self.wires.push(Wire {
            from_node: from_node.into(),
            from_output: from_output.into(),
            to_input: to_input.into(),
        });
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
    pub fn seed(mut self, s: u64) -> Self {
        self.seed = Some(s);
        self
    }
}

/// Graph of named nodes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunGraph {
    pub nodes: IndexMap<String, RunNode>,
}

impl RunGraph {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn node(mut self, name: &str, node: RunNode) -> Self {
        self.nodes.insert(name.into(), node);
        self
    }
    /// Structural validation: wires reference existing nodes, no cycles.
    pub fn validate(&self) -> Result<()> {
        for (name, n) in &self.nodes {
            for w in &n.wires {
                if !self.nodes.contains_key(&w.from_node) {
                    return Err(SimorphError::InvalidInput(format!(
                        "node `{name}` wires from unknown node `{}`",
                        w.from_node
                    )));
                }
            }
        }
        // cycle detection (DFS)
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            None,
            Temp,
            Done,
        }
        let mut marks: HashMap<&str, Mark> = self
            .nodes
            .keys()
            .map(|k| (k.as_str(), Mark::None))
            .collect();
        fn visit<'a>(
            g: &'a RunGraph,
            n: &'a str,
            marks: &mut HashMap<&'a str, Mark>,
        ) -> Result<()> {
            match marks[n] {
                Mark::Done => return Ok(()),
                Mark::Temp => {
                    return Err(SimorphError::InvalidInput(format!(
                        "cycle through node `{n}`"
                    )))
                }
                Mark::None => {}
            }
            marks.insert(n, Mark::Temp);
            for w in &g.nodes[n].wires {
                visit(g, w.from_node.as_str(), marks)?;
            }
            marks.insert(n, Mark::Done);
            Ok(())
        }
        for k in self.nodes.keys() {
            visit(self, k.as_str(), &mut marks)?;
        }
        Ok(())
    }
}

/// Live status of a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeEvent {
    pub node: String,
    pub component: String,
    pub state: TaskState,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Outcome of a graph run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphResult {
    pub results: IndexMap<String, TaskResult>,
    pub states: IndexMap<String, TaskState>,
    pub wall_time_ms: u64,
}

impl GraphResult {
    pub fn all_succeeded(&self) -> bool {
        self.states.values().all(|s| s.succeeded())
    }
}

/// The orchestrator.
#[derive(Clone)]
pub struct Orchestrator {
    components: ComponentSet,
    pool: ResourcePool,
    executor: Executor,
    metrics: Metrics,
    events: broadcast::Sender<NodeEvent>,
}

impl Orchestrator {
    pub fn new(components: ComponentSet, pool: ResourcePool) -> Self {
        let (events, _) = broadcast::channel(4096);
        Self {
            components,
            pool,
            executor: Executor::new(),
            metrics: Metrics::new(),
            events,
        }
    }

    pub fn with_metrics(mut self, m: Metrics) -> Self {
        self.metrics = m;
        self
    }

    pub fn components(&self) -> &ComponentSet {
        &self.components
    }
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }
    pub fn subscribe(&self) -> broadcast::Receiver<NodeEvent> {
        self.events.subscribe()
    }

    /// Run a graph.  `global_deadline` bounds the whole run; on expiry all
    /// remaining nodes are cancelled.
    pub async fn submit_graph(
        &self,
        graph: RunGraph,
        global_deadline: Option<Duration>,
    ) -> Result<GraphResult> {
        graph.validate()?;
        let start = std::time::Instant::now();
        let mut scheduler = Scheduler::new(self.pool);
        let mut ids: HashMap<String, TaskId> = HashMap::new();
        let mut names: HashMap<TaskId, String> = HashMap::new();
        let mut comps: HashMap<TaskId, String> = HashMap::new();
        // Nodes must be added in dependency order; compute a topological order.
        let order = topo_order(&graph)?;
        for name in order {
            let node = &graph.nodes[&name];
            let component = self.components.get(&node.component).ok_or_else(|| {
                SimorphError::NotFound(format!("component `{}` for node `{name}`", node.component))
            })?;
            let manifest = component.manifest().clone();
            let mut spec = TaskSpec::new(name.clone())
                .priority(node.priority)
                .retry(node.retry.clone())
                .resources(manifest.resource_requirements.clone());
            if spec.resources.cpu_cores <= 0.0 {
                spec.resources.cpu_cores = 1.0;
            }
            if let Some(t) = node.timeout_ms {
                spec = spec.timeout(Duration::from_millis(t));
            }
            for w in &node.wires {
                let dep = ids[&w.from_node];
                if !spec.deps.contains(&dep) {
                    spec = spec.depends_on(dep);
                }
            }
            let id = spec.id;
            ids.insert(name.clone(), id);
            names.insert(id, name.clone());
            comps.insert(id, node.component.clone());
            let executor = self.executor.clone();
            let metrics = self.metrics.clone();
            let node = node.clone();
            let wires_by_dep: Vec<(TaskId, Wire)> = node
                .wires
                .iter()
                .map(|w| (ids[&w.from_node], w.clone()))
                .collect();
            let body =
                task_fn(move |ctx| {
                    let component: SharedComponent = component.clone();
                    let executor = executor.clone();
                    let metrics = metrics.clone();
                    let node = node.clone();
                    let wires = wires_by_dep.clone();
                    async move {
                        let mut req = TaskRequest::new(component.manifest().id);
                        req.inputs = node.inputs.clone();
                        req.seed = node.seed;
                        req.fidelity = node.fidelity;
                        req.config = node.config.clone();
                        for (dep, w) in &wires {
                            let dep_result: TaskResult = serde_json::from_value(
                                ctx.inputs.get(dep).cloned().ok_or_else(|| {
                                    SimorphError::Other(format!(
                                        "missing upstream result from `{}`",
                                        w.from_node
                                    ))
                                })?,
                            )?;
                            let v = dep_result.outputs.get(&w.from_output).cloned().ok_or_else(
                                || {
                                    SimorphError::InvalidInput(format!(
                                        "node `{}` has no output `{}` (wired into `{}`)",
                                        w.from_node, w.from_output, w.to_input
                                    ))
                                },
                            )?;
                            req.inputs.insert(w.to_input.clone(), v);
                        }
                        let timeout = ctx.deadline.map(|d| d.remaining());
                        let labels: Labels =
                            [("component".to_string(), component.manifest().name.clone())]
                                .into_iter()
                                .collect();
                        let t0 = std::time::Instant::now();
                        let r = executor
                            .run(component, req, timeout, ctx.cancel.clone())
                            .await;
                        metrics.observe(
                            "simorph_task_duration_seconds",
                            &labels,
                            t0.elapsed().as_secs_f64(),
                        );
                        match &r {
                            Ok(_) => metrics.inc_by(
                                "simorph_tasks_total",
                                &with_status(&labels, "succeeded"),
                                1,
                            ),
                            Err(SimorphError::Timeout(_)) => metrics.inc_by(
                                "simorph_tasks_total",
                                &with_status(&labels, "timeout"),
                                1,
                            ),
                            Err(SimorphError::Cancelled) => metrics.inc_by(
                                "simorph_tasks_total",
                                &with_status(&labels, "cancelled"),
                                1,
                            ),
                            Err(_) => metrics.inc_by(
                                "simorph_tasks_total",
                                &with_status(&labels, "failed"),
                                1,
                            ),
                        }
                        Ok(serde_json::to_value(r?)?)
                    }
                });
            scheduler.add(spec, body)?;
        }
        // Forward scheduler events as node events.
        let mut rx: broadcast::Receiver<SchedulerEvent> = scheduler.subscribe();
        let events = self.events.clone();
        let names_c = names.clone();
        let comps_c = comps.clone();
        let forwarder = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        let _ = events.send(NodeEvent {
                            node: names_c.get(&ev.task).cloned().unwrap_or_default(),
                            component: comps_c.get(&ev.task).cloned().unwrap_or_default(),
                            state: ev.state,
                            timestamp: ev.timestamp,
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        // Global deadline.
        let cancel = scheduler.cancel_handle();
        let deadline_task = global_deadline.map(|d| {
            tokio::spawn(async move {
                tokio::time::sleep(d).await;
                tracing::warn!("global deadline reached: cancelling remaining nodes");
                cancel.cancel_all();
            })
        });
        let states = scheduler.run().await;
        if let Some(t) = deadline_task {
            t.abort();
        }
        drop(scheduler);
        // give the forwarder a moment to drain; it ends when the sender drops
        let _ = tokio::time::timeout(Duration::from_millis(200), forwarder).await;

        let mut results = IndexMap::new();
        let mut out_states = IndexMap::new();
        for (name, _) in &graph.nodes {
            let id = ids[name];
            let st = states[&id].clone();
            if let TaskState::Succeeded { output } = &st {
                let r: TaskResult = serde_json::from_value(output.clone())?;
                results.insert(name.clone(), r);
            }
            out_states.insert(name.clone(), st);
        }
        self.metrics.inc("simorph_graphs_total");
        Ok(GraphResult {
            results,
            states: out_states,
            wall_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}

fn with_status(labels: &Labels, status: &str) -> Labels {
    let mut l = labels.clone();
    l.insert("status".into(), status.into());
    l
}

fn topo_order(graph: &RunGraph) -> Result<Vec<String>> {
    let mut order = vec![];
    let mut done: HashMap<&str, bool> = HashMap::new();
    fn visit<'a>(
        g: &'a RunGraph,
        n: &'a str,
        done: &mut HashMap<&'a str, bool>,
        order: &mut Vec<String>,
    ) {
        if done.contains_key(n) {
            return;
        }
        done.insert(n, true);
        for w in &g.nodes[n].wires {
            visit(g, w.from_node.as_str(), done, order);
        }
        order.push(n.to_string());
    }
    for k in graph.nodes.keys() {
        visit(graph, k.as_str(), &mut done, &mut order);
    }
    Ok(order)
}

/// Restart budget for supervised components.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RestartPolicy {
    pub max_restarts: u32,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self { max_restarts: 3 }
    }
}

type Factory = Arc<dyn Fn() -> Result<SharedComponent> + Send + Sync>;

/// Wraps a component factory; when the live instance fails with a
/// `Component` error (crash, protocol violation, restart budget exhausted),
/// a fresh instance is created from the factory, up to `max_restarts`.
pub struct Supervisor {
    factory: Factory,
    policy: RestartPolicy,
    manifest: ComponentManifest,
    instance: Mutex<SharedComponent>,
    restarts: Mutex<u32>,
}

impl Supervisor {
    pub fn new(
        factory: impl Fn() -> Result<SharedComponent> + Send + Sync + 'static,
        policy: RestartPolicy,
    ) -> Result<Self> {
        let factory: Factory = Arc::new(factory);
        let instance = factory()?;
        let manifest = instance.manifest().clone();
        Ok(Self {
            factory,
            policy,
            manifest,
            instance: Mutex::new(instance),
            restarts: Mutex::new(0),
        })
    }

    pub fn restarts(&self) -> u32 {
        *self.restarts.lock()
    }

    fn restart(&self) -> Result<()> {
        let mut n = self.restarts.lock();
        if *n >= self.policy.max_restarts {
            return Err(SimorphError::Component {
                component: self.manifest.name.clone(),
                message: format!("restart budget of {} exhausted", self.policy.max_restarts),
            });
        }
        *n += 1;
        tracing::warn!(component = %self.manifest.name, restart = *n, "supervisor re-instantiating component");
        let fresh = (self.factory)()?;
        *self.instance.lock() = fresh;
        Ok(())
    }
}

impl Component for Supervisor {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        loop {
            let inst = self.instance.lock().clone();
            match inst.execute(req, ctl) {
                Err(e @ SimorphError::Component { .. })
                | Err(e @ SimorphError::Serialization(_)) => {
                    if let Err(budget) = self.restart() {
                        tracing::error!("{budget}; last error: {e}");
                        return Err(budget);
                    }
                }
                other => return other,
            }
        }
    }
}

/// Convenience: cancellation token shared by a whole run.
pub fn run_token() -> CancellationToken {
    CancellationToken::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::{Capability, Port};
    use simorph_plugin_sdk::ComponentBuilder;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn affine(name: &str, a: f64, b: f64) -> SharedComponent {
        Arc::new(
            ComponentBuilder::new(name, "1", "t")
                .capability(Capability::AlgebraicSolver)
                .input(Port::scalar("x", "1"))
                .output(Port::scalar("y", "1"))
                .build(move |req, _| {
                    let x = req.get_f64("x")?;
                    let mut out = IndexMap::new();
                    out.insert("y".into(), serde_json::json!(a * x + b));
                    Ok(out)
                })
                .unwrap(),
        )
    }

    fn adder() -> SharedComponent {
        Arc::new(
            ComponentBuilder::new("add", "1", "t")
                .input(Port::scalar("p", "1"))
                .input(Port::scalar("q", "1"))
                .output(Port::scalar("sum", "1"))
                .build(|req, _| {
                    let mut out = IndexMap::new();
                    out.insert(
                        "sum".into(),
                        serde_json::json!(req.get_f64("p")? + req.get_f64("q")?),
                    );
                    Ok(out)
                })
                .unwrap(),
        )
    }

    fn failing() -> SharedComponent {
        Arc::new(
            ComponentBuilder::new("boom", "1", "t")
                .input(Port::scalar("x", "1"))
                .output(Port::scalar("y", "1"))
                .build(|_, _| Err(SimorphError::Numerical("diverged".into())))
                .unwrap(),
        )
    }

    fn set() -> ComponentSet {
        let s = ComponentSet::new();
        s.register_instance(affine("double", 2.0, 0.0)).unwrap();
        s.register_instance(affine("plus_one", 1.0, 1.0)).unwrap();
        s.register_instance(adder()).unwrap();
        s.register_instance(failing()).unwrap();
        s
    }

    #[tokio::test]
    async fn diamond_graph_passes_outputs() {
        let orch = Orchestrator::new(
            set(),
            ResourcePool {
                cpu_cores: 4.0,
                memory_mb: 4096,
                gpus: 0,
            },
        );
        let mut rx = orch.subscribe();
        let g = RunGraph::new()
            .node("src", RunNode::new("double").input("x", 3.0))
            .node("left", RunNode::new("double").wire("src", "y", "x"))
            .node("right", RunNode::new("plus_one").wire("src", "y", "x"))
            .node(
                "sink",
                RunNode::new("add")
                    .wire("left", "y", "p")
                    .wire("right", "y", "q"),
            );
        let r = orch
            .submit_graph(g, Some(Duration::from_secs(10)))
            .await
            .unwrap();
        assert!(r.all_succeeded(), "{:?}", r.states);
        // src: 6, left: 12, right: 7, sink: 19
        assert_eq!(r.results["sink"].get_f64("sum").unwrap(), 19.0);
        let mut running = 0;
        let mut succeeded = 0;
        while let Ok(ev) = rx.try_recv() {
            match ev.state {
                TaskState::Running { .. } => running += 1,
                TaskState::Succeeded { .. } => succeeded += 1,
                _ => {}
            }
        }
        assert_eq!(running, 4);
        assert_eq!(succeeded, 4);
        let snap = orch.metrics().snapshot();
        assert_eq!(snap.counters["simorph_graphs_total"], 1);
        assert_eq!(
            snap.counters["simorph_tasks_total{component=double,status=succeeded}"],
            2
        );
    }

    #[tokio::test]
    async fn failing_node_skips_dependents_only() {
        let orch = Orchestrator::new(
            set(),
            ResourcePool {
                cpu_cores: 2.0,
                memory_mb: 4096,
                gpus: 0,
            },
        );
        let g = RunGraph::new()
            .node("bad", RunNode::new("boom").input("x", 1.0))
            .node("after", RunNode::new("double").wire("bad", "y", "x"))
            .node("free", RunNode::new("double").input("x", 1.0));
        let r = orch.submit_graph(g, None).await.unwrap();
        assert!(
            matches!(r.states["bad"], TaskState::Failed { ref error, .. } if error.contains("diverged"))
        );
        assert!(matches!(r.states["after"], TaskState::Skipped { .. }));
        assert!(r.states["free"].succeeded());
        assert_eq!(r.results.len(), 1);
    }

    #[tokio::test]
    async fn invalid_graphs_are_rejected() {
        let orch = Orchestrator::new(
            set(),
            ResourcePool {
                cpu_cores: 2.0,
                memory_mb: 4096,
                gpus: 0,
            },
        );
        let g = RunGraph::new().node("a", RunNode::new("double").wire("zzz", "y", "x"));
        assert!(orch.submit_graph(g, None).await.is_err());
        let g = RunGraph::new()
            .node("a", RunNode::new("double").wire("b", "y", "x"))
            .node("b", RunNode::new("double").wire("a", "y", "x"));
        assert!(
            matches!(orch.submit_graph(g, None).await, Err(SimorphError::InvalidInput(m)) if m.contains("cycle"))
        );
        let g = RunGraph::new().node("a", RunNode::new("missing").input("x", 1.0));
        assert!(matches!(
            orch.submit_graph(g, None).await,
            Err(SimorphError::NotFound(_))
        ));
        // wiring a non-existent output fails that node at run time
        let g = RunGraph::new()
            .node("a", RunNode::new("double").input("x", 1.0))
            .node("b", RunNode::new("double").wire("a", "nope", "x"));
        let r = orch.submit_graph(g, None).await.unwrap();
        assert!(matches!(r.states["b"], TaskState::Failed { .. }));
    }

    #[tokio::test]
    async fn global_deadline_cancels_remaining_nodes() {
        let s = ComponentSet::new();
        s.register_instance(Arc::new(
            ComponentBuilder::new("slow", "1", "t")
                .input(Port::scalar("x", "1"))
                .output(Port::scalar("y", "1"))
                .build(|_, ctl| {
                    let t0 = std::time::Instant::now();
                    while t0.elapsed() < Duration::from_secs(5) {
                        ctl.check()?;
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Ok(IndexMap::new())
                })
                .unwrap(),
        ))
        .unwrap();
        let orch = Orchestrator::new(
            s,
            ResourcePool {
                cpu_cores: 1.0,
                memory_mb: 4096,
                gpus: 0,
            },
        );
        let g = RunGraph::new()
            .node("a", RunNode::new("slow").input("x", 1.0))
            .node("b", RunNode::new("slow").wire("a", "y", "x"));
        let t0 = std::time::Instant::now();
        let r = orch
            .submit_graph(g, Some(Duration::from_millis(100)))
            .await
            .unwrap();
        assert!(t0.elapsed() < Duration::from_secs(3));
        assert_eq!(r.states["a"], TaskState::Cancelled);
        assert!(matches!(
            r.states["b"],
            TaskState::Skipped { .. } | TaskState::Cancelled
        ));
    }

    #[test]
    fn supervisor_reinstantiates_crashed_component() {
        let builds = Arc::new(AtomicU32::new(0));
        let b = builds.clone();
        let sup = Supervisor::new(
            move || {
                let n = b.fetch_add(1, Ordering::SeqCst);
                let c: SharedComponent = Arc::new(
                    ComponentBuilder::new("crashy", "1", "t")
                        .input(Port::scalar("x", "1"))
                        .output(Port::scalar("y", "1"))
                        .build(move |req, _| {
                            if n < 2 {
                                Err(SimorphError::Component {
                                    component: "crashy".into(),
                                    message: "crash".into(),
                                })
                            } else {
                                let mut out = IndexMap::new();
                                out.insert("y".into(), serde_json::json!(req.get_f64("x")?));
                                Ok(out)
                            }
                        })
                        .unwrap(),
                );
                Ok(c)
            },
            RestartPolicy { max_restarts: 3 },
        )
        .unwrap();
        let req = TaskRequest::new(sup.manifest().id).input("x", 9.0);
        let r = simorph_core::run_component(&sup, &req, &ExecutionControl::default()).unwrap();
        assert_eq!(r.get_f64("y").unwrap(), 9.0);
        assert_eq!(sup.restarts(), 2);
        assert_eq!(builds.load(Ordering::SeqCst), 3);

        // budget exhaustion
        let sup = Supervisor::new(
            || Ok(failing_component()),
            RestartPolicy { max_restarts: 1 },
        )
        .unwrap();
        let req = TaskRequest::new(sup.manifest().id).input("x", 1.0);
        let e = simorph_core::run_component(&sup, &req, &ExecutionControl::default()).unwrap_err();
        assert!(e.to_string().contains("restart budget"), "{e}");
    }

    fn failing_component() -> SharedComponent {
        Arc::new(
            ComponentBuilder::new("always_crash", "1", "t")
                .input(Port::scalar("x", "1"))
                .output(Port::scalar("y", "1"))
                .build(|_, _| {
                    Err(SimorphError::Component {
                        component: "always_crash".into(),
                        message: "crash".into(),
                    })
                })
                .unwrap(),
        )
    }
}
