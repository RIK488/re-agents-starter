//! Out-of-process components.

use parking_lot::Mutex;
use simorph_core::{
    Component, ComponentManifest, ExecutionControl, Result, SimorphError, TaskRequest, TaskResult,
};
use simorph_plugin_sdk::protocol::{Request, Response};
use simorph_security::SandboxPolicy;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Counters exposed for supervision.
#[derive(Debug, Default)]
pub struct SubprocessStats {
    pub spawns: AtomicU64,
    pub kills: AtomicU64,
    pub executions: AtomicU64,
    pub failures: AtomicU64,
}

struct Child_ {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<std::io::Result<String>>,
    /// Bytes read from stdout since spawn.
    bytes: Arc<AtomicU64>,
}

impl Child_ {
    fn kill(&mut self) {
        // The child leads its own process group (see `isolation`): kill the
        // whole group so that grandchildren cannot outlive a timeout.
        crate::isolation::kill_group(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A component implemented by an external program.
///
/// The process is kept alive between executions (warm interpreter); it is
/// killed and respawned on timeout, protocol violation, output overflow or
/// crash.  Restarts are counted and capped by `max_restarts`.
pub struct SubprocessComponent {
    command: String,
    args: Vec<String>,
    policy: SandboxPolicy,
    manifest: ComponentManifest,
    child: Mutex<Option<Child_>>,
    restarts: AtomicU64,
    max_restarts: u64,
    pub stats: SubprocessStats,
}

impl std::fmt::Debug for SubprocessComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubprocessComponent")
            .field("command", &self.command)
            .field("args", &self.args)
            .field("manifest", &self.manifest.name)
            .finish()
    }
}

impl SubprocessComponent {
    /// Spawn the program once to fetch its manifest.  Fails if the program
    /// cannot start, violates the protocol, or exceeds the policy timeout.
    pub fn new(
        command: impl Into<String>,
        args: Vec<String>,
        policy: SandboxPolicy,
    ) -> Result<Self> {
        policy.validate()?;
        let mut this = Self {
            command: command.into(),
            args,
            policy,
            manifest: ComponentManifest::in_process("pending", "0", "pending"),
            child: Mutex::new(None),
            restarts: AtomicU64::new(0),
            max_restarts: 3,
            stats: SubprocessStats::default(),
        };
        let manifest = this.fetch_manifest()?;
        manifest.validate_structure()?;
        this.manifest = manifest;
        Ok(this)
    }

    pub fn with_max_restarts(mut self, n: u64) -> Self {
        self.max_restarts = n;
        self
    }

    pub fn restarts(&self) -> u64 {
        self.restarts.load(Ordering::Relaxed)
    }

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    fn timeout(&self) -> Option<Duration> {
        if self.policy.max_cpu_s > 0.0 {
            Some(Duration::from_secs_f64(self.policy.max_cpu_s))
        } else {
            None
        }
    }

    fn spawn(&self) -> Result<Child_> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .env_clear()
            .envs(self.policy.to_subprocess_env())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(wd) = &self.policy.working_dir {
            cmd.current_dir(wd);
        }
        if self.policy.require_os_isolation && !crate::isolation::supported() {
            return Err(SimorphError::PermissionDenied(
                "OS isolation required by the sandbox policy is not supported on this platform"
                    .into(),
            ));
        }
        crate::isolation::apply(
            &mut cmd,
            crate::isolation::OsLimits::from_policy(&self.policy),
        );
        let mut child = cmd.spawn().map_err(|e| SimorphError::Component {
            component: self.command.clone(),
            message: format!("spawn failed (sandbox isolation included): {e}"),
        })?;
        let stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let (tx, rx) = mpsc::channel();
        let bytes = Arc::new(AtomicU64::new(0));
        let counter = bytes.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(l) = &line {
                    counter.fetch_add(l.len() as u64 + 1, Ordering::Relaxed);
                }
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        self.stats.spawns.fetch_add(1, Ordering::Relaxed);
        Ok(Child_ {
            child,
            stdin,
            lines: rx,
            bytes,
        })
    }

    /// Ensure a live child, respawning if needed (and allowed).
    fn ensure_child<'a>(&self, slot: &'a mut Option<Child_>) -> Result<&'a mut Child_> {
        let dead = match slot {
            Some(c) => c.child.try_wait().map(|s| s.is_some()).unwrap_or(true),
            None => true,
        };
        if dead {
            if self.stats.spawns.load(Ordering::Relaxed) > 0 {
                let n = self.restarts.fetch_add(1, Ordering::Relaxed) + 1;
                if n > self.max_restarts {
                    return Err(SimorphError::Component {
                        component: self.manifest.name.clone(),
                        message: format!(
                            "subprocess restarted {n} times, exceeding the limit of {}",
                            self.max_restarts
                        ),
                    });
                }
                tracing::warn!(component = %self.manifest.name, restart = n, "respawning subprocess component");
            }
            *slot = Some(self.spawn()?);
        }
        Ok(slot.as_mut().unwrap())
    }

    /// Send one request and wait for its terminal response, honouring the
    /// policy timeout, the output cap and the execution control.
    fn round_trip(&self, req: &Request, ctl: Option<&ExecutionControl>) -> Result<Response> {
        let mut guard = self.child.lock();
        let child = self.ensure_child(&mut guard)?;
        let line = serde_json::to_string(req)?;
        if let Err(e) = writeln!(child.stdin, "{line}").and_then(|_| child.stdin.flush()) {
            // A broken pipe means the child is gone: report its exit status
            // rather than the symptom (same wording as the read path).
            let status = child.child.try_wait().ok().flatten();
            child.kill();
            *guard = None;
            return Err(SimorphError::Component {
                component: self.manifest.name.clone(),
                message: match status {
                    Some(st) => {
                        format!("subprocess exited before accepting the request (status {st:?})")
                    }
                    None => format!("write failed: {e}"),
                },
            });
        }
        let start = Instant::now();
        let timeout = self.timeout();
        let name = self.manifest.name.clone();
        loop {
            if let Some(c) = ctl {
                if let Err(e) = c.check() {
                    let _ = writeln!(child.stdin, "{}", serde_json::to_string(&Request::Cancel)?);
                    child.kill();
                    self.stats.kills.fetch_add(1, Ordering::Relaxed);
                    *guard = None;
                    return Err(e);
                }
            }
            if let Some(t) = timeout {
                if start.elapsed() >= t {
                    child.kill();
                    self.stats.kills.fetch_add(1, Ordering::Relaxed);
                    *guard = None;
                    return Err(SimorphError::Timeout(t.as_millis() as u64));
                }
            }
            if child.bytes.load(Ordering::Relaxed) > self.policy.max_output_bytes {
                child.kill();
                self.stats.kills.fetch_add(1, Ordering::Relaxed);
                *guard = None;
                return Err(SimorphError::Component {
                    component: name,
                    message: format!("output exceeded {} bytes", self.policy.max_output_bytes),
                });
            }
            match child.lines.recv_timeout(Duration::from_millis(20)) {
                Ok(Ok(l)) => {
                    // Re-check the cap after the line is counted: a single
                    // oversized line must be reported as an overflow, not
                    // parsed (and not mis-reported as a protocol violation).
                    if child.bytes.load(Ordering::Relaxed) > self.policy.max_output_bytes {
                        child.kill();
                        self.stats.kills.fetch_add(1, Ordering::Relaxed);
                        *guard = None;
                        return Err(SimorphError::Component {
                            component: name,
                            message: format!(
                                "output exceeded {} bytes",
                                self.policy.max_output_bytes
                            ),
                        });
                    }
                    if l.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Response>(&l) {
                        Ok(Response::Progress { fraction, message }) => {
                            tracing::debug!(component = %name, fraction, message = ?message, "progress");
                        }
                        Ok(r) => return Ok(r),
                        Err(e) => {
                            child.kill();
                            self.stats.kills.fetch_add(1, Ordering::Relaxed);
                            *guard = None;
                            let preview: String = l.chars().take(120).collect();
                            return Err(SimorphError::Serialization(format!(
                                "protocol violation from `{name}`: {e} (line: {preview:?})"
                            )));
                        }
                    }
                }
                Ok(Err(e)) => {
                    child.kill();
                    *guard = None;
                    return Err(SimorphError::Component {
                        component: name,
                        message: format!("read failed: {e}"),
                    });
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    let status = child.child.wait().ok();
                    *guard = None;
                    return Err(SimorphError::Component {
                        component: name,
                        message: format!(
                            "subprocess exited without responding (status {status:?})"
                        ),
                    });
                }
            }
        }
    }

    fn fetch_manifest(&self) -> Result<ComponentManifest> {
        match self.round_trip(&Request::Manifest, None)? {
            Response::Manifest { manifest } => Ok(*manifest),
            Response::Error { message, .. } => Err(SimorphError::Component {
                component: self.command.clone(),
                message,
            }),
            other => Err(SimorphError::Serialization(format!(
                "expected manifest, got {other:?}"
            ))),
        }
    }

    /// Ask the child to exit and reap it.
    pub fn shutdown(&self) {
        let mut guard = self.child.lock();
        if let Some(mut c) = guard.take() {
            let _ = writeln!(
                c.stdin,
                "{}",
                serde_json::to_string(&Request::Shutdown).unwrap()
            );
            let _ = c.stdin.flush();
            let deadline = Instant::now() + Duration::from_millis(500);
            while Instant::now() < deadline {
                if c.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            c.kill();
        }
    }
}

impl Drop for SubprocessComponent {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Component for SubprocessComponent {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }

    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        self.stats.executions.fetch_add(1, Ordering::Relaxed);
        let resp = self
            .round_trip(
                &Request::Execute {
                    request: req.clone(),
                },
                Some(ctl),
            )
            .map_err(|e| {
                self.stats.failures.fetch_add(1, Ordering::Relaxed);
                e
            })?;
        match resp {
            Response::Result { mut result } => {
                // The parent, not the child, is authoritative on these fields.
                result.run_id = req.run_id;
                result.component = self.manifest.id;
                result.request_hash = req.content_hash();
                Ok(result)
            }
            Response::Error { message, .. } => {
                self.stats.failures.fetch_add(1, Ordering::Relaxed);
                Err(SimorphError::Component {
                    component: self.manifest.name.clone(),
                    message,
                })
            }
            other => {
                self.stats.failures.fetch_add(1, Ordering::Relaxed);
                Err(SimorphError::Serialization(format!(
                    "expected result, got {other:?}"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::run_component;
    use std::path::PathBuf;

    fn tmp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("simorph-exec-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Minimal Python component speaking the protocol (no simorph_adapter
    /// package needed, so this test only depends on python3).
    const PY: &str = r#"
import sys, json, os
MANIFEST = {
  "id": "00000000-0000-0000-0000-000000000001", "name": "py.square", "version": "1.0.0",
  "provider": "test", "license": "MIT", "capabilities": ["algebraic_solver"], "supported_domains": ["test"],
  "input_schema": [{"name": "x", "unit": "1", "shape": []}],
  "output_schema": [{"name": "y", "unit": "1", "shape": []}, {"name": "env_leak", "unit": "1", "shape": []}],
  "uncertainty_model": {"kind": "none"}, "numerical_precision": "float64", "deterministic_mode": "always",
  "execution_interface": {"kind": "subprocess", "command": "python3"}
}
for line in sys.stdin:
    msg = json.loads(line)
    t = msg["type"]
    if t == "manifest":
        print(json.dumps({"type": "manifest", "manifest": MANIFEST}), flush=True)
    elif t == "execute":
        r = msg["request"]
        x = r["inputs"]["x"]
        if r.get("config") == "big":
            print(json.dumps({"type": "progress", "fraction": 0.5}), flush=True)
            print("x" * 5000, flush=True)
            continue
        if x < 0:
            print(json.dumps({"type": "error", "message": "negative input"}), flush=True)
            continue
        print(json.dumps({"type": "progress", "fraction": 0.5}), flush=True)
        print(json.dumps({"type": "result", "result": {
            "run_id": r["run_id"], "component": r["component"],
            "outputs": {"y": x * x, "env_leak": 1.0 if "SECRET_TOKEN" in os.environ else 0.0},
            "metrics": {}, "evidence": "computed", "request_hash": "", "warnings": [], "wall_time_ms": 0}}), flush=True)
    elif t == "shutdown":
        print(json.dumps({"type": "ack"}), flush=True)
        break
"#;

    fn python_component(policy: SandboxPolicy) -> Result<SubprocessComponent> {
        let dir = tmp_dir();
        let script = dir.join("square.py");
        std::fs::write(&script, PY).unwrap();
        SubprocessComponent::new(
            "python3",
            vec!["-I".into(), script.to_string_lossy().to_string()],
            policy.with_working_dir(dir),
        )
    }

    #[test]
    fn python_round_trip_with_progress_errors_and_env_scrubbing() {
        std::env::set_var("SECRET_TOKEN", "hunter2");
        let c = python_component(SandboxPolicy::strict().with_timeout(20.0)).unwrap();
        assert_eq!(c.manifest().name, "py.square");
        let req = TaskRequest::new(c.manifest().id).input("x", 3.0);
        let r = run_component(&c, &req, &ExecutionControl::default()).unwrap();
        assert_eq!(r.get_f64("y").unwrap(), 9.0);
        assert_eq!(
            r.get_f64("env_leak").unwrap(),
            0.0,
            "secret must not reach the child"
        );
        assert_eq!(r.request_hash, req.content_hash());
        // error path, same warm process
        let bad = TaskRequest::new(c.manifest().id).input("x", -1.0);
        assert!(matches!(
            run_component(&c, &bad, &ExecutionControl::default()),
            Err(SimorphError::Component { .. })
        ));
        assert_eq!(c.stats.spawns.load(Ordering::Relaxed), 1);
        // output cap: child killed and respawned transparently
        let big = TaskRequest::new(c.manifest().id)
            .input("x", 1.0)
            .config(serde_json::json!("big"));
        let small_cap = python_component(
            SandboxPolicy::strict()
                .with_timeout(20.0)
                .with_max_output(1024),
        )
        .unwrap();
        let big_req = TaskRequest {
            component: small_cap.manifest().id,
            ..big
        };
        let err = run_component(&small_cap, &big_req, &ExecutionControl::default()).unwrap_err();
        assert!(err.to_string().contains("output exceeded"), "{err}");
        let again = TaskRequest::new(small_cap.manifest().id).input("x", 2.0);
        assert_eq!(
            run_component(&small_cap, &again, &ExecutionControl::default())
                .unwrap()
                .get_f64("y")
                .unwrap(),
            4.0
        );
        assert_eq!(small_cap.restarts(), 1);
    }

    #[test]
    fn malformed_output_is_a_protocol_violation() {
        let r = SubprocessComponent::new(
            "sh",
            vec!["-c".into(), "echo not-json; cat >/dev/null".into()],
            SandboxPolicy::strict().with_timeout(5.0),
        );
        match r {
            Err(SimorphError::Serialization(m)) => assert!(m.contains("protocol violation"), "{m}"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn exiting_child_is_reported() {
        let r = SubprocessComponent::new(
            "sh",
            vec!["-c".into(), "exit 3".into()],
            SandboxPolicy::strict().with_timeout(5.0),
        );
        match r {
            Err(SimorphError::Component { message, .. }) => {
                assert!(message.contains("exited"), "{message}")
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn timeout_kills_child() {
        let start = Instant::now();
        let r = SubprocessComponent::new(
            "sh",
            vec!["-c".into(), "sleep 30".into()],
            SandboxPolicy::strict().with_timeout(0.3),
        );
        assert!(matches!(r, Err(SimorphError::Timeout(_))), "{r:?}");
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn cancellation_kills_child() {
        let c = python_component(SandboxPolicy::strict().with_timeout(30.0)).unwrap();
        let ctl = ExecutionControl::default();
        ctl.cancel.cancel();
        let req = TaskRequest::new(c.manifest().id).input("x", 1.0);
        assert!(matches!(
            run_component(&c, &req, &ctl),
            Err(SimorphError::Cancelled)
        ));
        assert_eq!(c.stats.kills.load(Ordering::Relaxed), 1);
    }
}
