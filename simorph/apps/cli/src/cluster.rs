//! Distributed execution of campaigns (subsystem 16 on the main path).
//!
//! A [`LocalCluster`] starts an in-process coordinator and `N` worker
//! processes (`simorph worker`), each serving the laboratory component set.
//! [`remote_component_set`] wraps every component as a [`RemoteComponent`]
//! whose `execute` is a task submitted to the coordinator, so any campaign —
//! screening, optimisation, verification — runs on the workers unchanged.
//! Workers authenticate with a random per-cluster HMAC secret passed through
//! an environment variable.

use simorph_core::*;
use simorph_distributed::{
    Coordinator, CoordinatorConfig, CoordinatorMetrics, OutcomeKind, TaskSpec,
};
use simorph_plugin_sdk::ComponentSet;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

pub const WORKER_SECRET_ENV: &str = "SIMORPH_WORKER_SECRET";

/// A component executed on the cluster.
pub struct RemoteComponent {
    manifest: ComponentManifest,
    coordinator: Arc<Coordinator>,
    timeout: Duration,
}

impl Component for RemoteComponent {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        ctl.check()?;
        let spec = TaskSpec::new(&self.manifest.name, req.clone())
            .requirements(self.manifest.resource_requirements.clone());
        let id = self.coordinator.submit(spec)?;
        let out = self.coordinator.wait(id, self.timeout)?;
        match out.kind {
            OutcomeKind::Done => {
                let mut r = out
                    .result
                    .ok_or_else(|| SimorphError::Other("cluster returned no result".into()))?;
                // De-duplicated submissions share one remote run; the caller's
                // run id is restored so provenance stays one record per run.
                r.run_id = req.run_id;
                r.metrics
                    .insert("cluster_attempts".into(), out.attempts as f64);
                Ok(r)
            }
            OutcomeKind::Cancelled => Err(SimorphError::Cancelled),
            _ => Err(SimorphError::Component {
                component: self.manifest.name.clone(),
                message: format!(
                    "cluster task {:?}: {}",
                    out.kind,
                    out.error.unwrap_or_default()
                ),
            }),
        }
    }
}

/// Wrap every component of `local` as a remote component of `coordinator`.
pub fn remote_component_set(
    local: &ComponentSet,
    coordinator: Arc<Coordinator>,
    timeout: Duration,
) -> Result<ComponentSet> {
    let set = ComponentSet::new();
    for m in local.manifests() {
        let c = RemoteComponent {
            manifest: m,
            coordinator: coordinator.clone(),
            timeout,
        };
        set.register_instance(Arc::new(c))?;
    }
    Ok(set)
}

/// Coordinator + worker processes on this machine.
pub struct LocalCluster {
    pub coordinator: Arc<Coordinator>,
    workers: Vec<Child>,
}

impl LocalCluster {
    /// Start `n` workers with `slots` concurrent tasks each.
    pub fn start(n: usize, slots: usize) -> Result<Self> {
        let secret = simorph_core::hashing::sha256_hex(RunId::new().to_string().as_bytes());
        let cfg = CoordinatorConfig {
            secret: Some(secret.as_bytes().to_vec()),
            ..CoordinatorConfig::default()
        };
        let coordinator = Arc::new(Coordinator::bind("127.0.0.1:0", cfg)?);
        let exe = worker_executable()?;
        let addr = coordinator.addr().to_string();
        let mut workers = vec![];
        for i in 0..n {
            let child = Command::new(&exe)
                .args([
                    "worker",
                    "--connect",
                    &addr,
                    "--id",
                    &format!("w{i}"),
                    "--slots",
                    &slots.to_string(),
                ])
                .env(WORKER_SECRET_ENV, &secret)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|e| {
                    SimorphError::Io(format!("cannot start worker `{}`: {e}", exe.display()))
                })?;
            workers.push(child);
        }
        let cluster = Self {
            coordinator,
            workers,
        };
        cluster
            .coordinator
            .wait_for_workers(n, Duration::from_secs(120))?;
        Ok(cluster)
    }

    pub fn metrics(&self) -> CoordinatorMetrics {
        self.coordinator.metrics()
    }
}

impl Drop for LocalCluster {
    fn drop(&mut self) {
        self.coordinator.shutdown();
        for w in &mut self.workers {
            let _ = w.kill();
            let _ = w.wait();
        }
    }
}

/// The `simorph` executable that hosts `simorph worker` (this binary, or
/// `SIMORPH_BIN` when called from a library context such as tests).
fn worker_executable() -> Result<std::path::PathBuf> {
    if let Ok(p) = std::env::var("SIMORPH_BIN") {
        return Ok(p.into());
    }
    let exe = std::env::current_exe()?;
    if exe.file_stem().map(|s| s == "simorph").unwrap_or(false) {
        return Ok(exe);
    }
    Err(SimorphError::NotFound(
        "simorph executable for workers (set SIMORPH_BIN)".into(),
    ))
}

/// Serve the laboratory component set to a coordinator (worker process).
pub fn serve_worker(
    connect: &str,
    id: &str,
    slots: Option<usize>,
) -> Result<simorph_distributed::WorkerExit> {
    let mut cfg = simorph_distributed::WorkerConfig::new(id);
    cfg.secret = std::env::var(WORKER_SECRET_ENV)
        .ok()
        .map(|s| s.into_bytes());
    cfg.slots = slots;
    simorph_distributed::run_worker(connect, cfg, crate::registry::component_set()?)
}
