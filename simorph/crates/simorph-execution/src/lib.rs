//! # simorph-execution
//!
//! * [`Executor`] runs any [`Component`] either synchronously or on the
//!   Tokio blocking pool with a wall-clock timeout mapped to
//!   `SimorphError::Timeout` and cooperative cancellation.
//! * [`SubprocessComponent`] wraps an external program speaking the SIMORPH
//!   JSON-lines protocol (`simorph_plugin_sdk::protocol`).  It enforces the
//!   parts of [`SandboxPolicy`] a parent process can enforce: timeout (kill),
//!   output byte cap (kill), environment scrubbing and working directory.
//!   Network/filesystem/memory isolation is delegated to the container layer
//!   (see `SandboxPolicy::to_container_args`).

pub mod isolation;
pub mod subprocess;

pub use subprocess::{SubprocessComponent, SubprocessStats};

use simorph_core::{
    run_component, CancellationToken, Component, Deadline, ExecutionControl, Result, SimorphError,
    TaskRequest, TaskResult,
};
use std::sync::Arc;
use std::time::Duration;

/// Component runner.
#[derive(Debug, Clone, Default)]
pub struct Executor;

impl Executor {
    pub fn new() -> Self {
        Self
    }

    /// Synchronous execution with compatibility check, timing and control.
    pub fn run_blocking(
        &self,
        component: &dyn Component,
        req: &TaskRequest,
        ctl: &ExecutionControl,
    ) -> Result<TaskResult> {
        run_component(component, req, ctl)
    }

    /// Asynchronous execution on the blocking pool with a hard timeout.
    /// On timeout the cancellation token is set (so well-behaved components
    /// stop) and `SimorphError::Timeout` is returned immediately.
    pub async fn run(
        &self,
        component: Arc<dyn Component>,
        req: TaskRequest,
        timeout: Option<Duration>,
        cancel: CancellationToken,
    ) -> Result<TaskResult> {
        let ctl = ExecutionControl {
            cancel: cancel.clone(),
            deadline: timeout.map(Deadline::after),
        };
        let handle =
            tokio::task::spawn_blocking(move || run_component(component.as_ref(), &req, &ctl));
        match timeout {
            Some(t) => match tokio::time::timeout(t, handle).await {
                Ok(joined) => {
                    let r =
                        joined.map_err(|e| SimorphError::Other(format!("task panicked: {e}")))?;
                    // The component may observe its own deadline before the
                    // outer timer fires; a timed-out task is cancelled either way.
                    if let Err(SimorphError::Timeout(_)) = &r {
                        cancel.cancel();
                        return Err(SimorphError::Timeout(t.as_millis() as u64));
                    }
                    r
                }
                Err(_) => {
                    cancel.cancel();
                    Err(SimorphError::Timeout(t.as_millis() as u64))
                }
            },
            None => handle
                .await
                .map_err(|e| SimorphError::Other(format!("task panicked: {e}")))?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::{Capability, Port};
    use simorph_plugin_sdk::ComponentBuilder;

    fn slow() -> Arc<dyn Component> {
        Arc::new(
            ComponentBuilder::new("test.slow", "1", "t")
                .capability(Capability::Other("sleep".into()))
                .input(Port::scalar("ms", "ms"))
                .output(Port::scalar("done", "1"))
                .build(|req, ctl| {
                    let ms = req.get_f64("ms")? as u64;
                    let start = std::time::Instant::now();
                    while start.elapsed().as_millis() < ms as u128 {
                        ctl.check()?;
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    let mut out = indexmap::IndexMap::new();
                    out.insert("done".into(), serde_json::json!(1.0));
                    Ok(out)
                })
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn timeout_maps_to_error_and_cancels() {
        let ex = Executor::new();
        let c = slow();
        let req = TaskRequest::new(c.manifest().id).input("ms", 2000.0);
        let cancel = CancellationToken::new();
        let r = ex
            .run(c, req, Some(Duration::from_millis(100)), cancel.clone())
            .await;
        assert!(matches!(r, Err(SimorphError::Timeout(_))), "{r:?}");
        assert!(cancel.is_cancelled());
    }

    #[tokio::test]
    async fn fast_task_completes() {
        let ex = Executor::new();
        let c = slow();
        let req = TaskRequest::new(c.manifest().id).input("ms", 10.0);
        let r = ex
            .run(
                c,
                req,
                Some(Duration::from_secs(5)),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(r.get_f64("done").unwrap(), 1.0);
        assert!(r.metrics.contains_key("wall_time_ms"));
    }

    #[test]
    fn blocking_cancellation() {
        let ex = Executor::new();
        let c = slow();
        let req = TaskRequest::new(c.manifest().id).input("ms", 5000.0);
        let ctl = ExecutionControl::default();
        let cancel = ctl.cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            cancel.cancel();
        });
        let r = ex.run_blocking(c.as_ref(), &req, &ctl);
        assert!(matches!(r, Err(SimorphError::Cancelled)));
    }
}
