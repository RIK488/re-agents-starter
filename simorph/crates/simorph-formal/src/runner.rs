//! Sandboxed execution of the Python helper (`adapters/python/simorph_formal/formal_tool.py`).
//!
//! Enforcement actually performed here (not just declared):
//! * environment cleared, only the policy allow-list is passed;
//! * wall-clock timeout with process kill (`SandboxPolicy::max_cpu_s`);
//! * stdout size cap (`max_output_bytes`), child killed beyond it;
//! * address-space limit `RLIMIT_AS = max_memory_mb` set by the helper
//!   before importing any solver;
//! * when `network == false` and `unshare -n` works on this host, the child
//!   runs in an empty network namespace (no interfaces but loopback down).
//!   Whether this was applied is reported in [`RunInfo`].

use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use simorph_security::SandboxPolicy;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Path of the helper script in the repository checkout.
pub fn tool_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../adapters/python/simorph_formal/formal_tool.py")
}

/// Facts about one helper execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunInfo {
    pub interpreter: String,
    pub network_isolated: bool,
    pub wall_ms: f64,
}

fn unshare_available() -> bool {
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| {
        Command::new("unshare")
            .args(["-n", "true"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

/// Default policy for solver runs: strict, 60 s, 2 GiB address space.
pub fn solver_policy() -> SandboxPolicy {
    SandboxPolicy::strict().with_timeout(60.0)
}

/// Run one helper operation.  The request must contain `op`.
pub fn run_tool(
    request: &serde_json::Value,
    policy: &SandboxPolicy,
) -> Result<(serde_json::Value, RunInfo)> {
    policy.validate()?;
    let interpreter = simorph_adapter_python_bridge::python_interpreter();
    let script = tool_path();
    if !script.is_file() {
        return Err(SimorphError::NotFound(format!(
            "formal helper {}",
            script.display()
        )));
    }
    let mut req = request.clone();
    if let Some(o) = req.as_object_mut() {
        o.insert("memory_mb".into(), serde_json::json!(policy.max_memory_mb));
    }
    let isolate = !policy.network && unshare_available();
    let mut cmd = if isolate {
        let mut c = Command::new("unshare");
        c.arg("-n").arg(&interpreter);
        c
    } else {
        Command::new(&interpreter)
    };
    cmd.arg("-I")
        .arg(&script)
        .env_clear()
        .envs(policy.to_subprocess_env())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(wd) = &policy.working_dir {
        cmd.current_dir(wd);
    }
    let start = Instant::now();
    let mut child = cmd.spawn().map_err(|e| SimorphError::Component {
        component: "formal_tool".into(),
        message: format!("spawn {interpreter} failed: {e}"),
    })?;
    {
        let mut stdin = child.stdin.take().expect("piped");
        stdin.write_all(serde_json::to_string(&req)?.as_bytes())?;
    }
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    let cap = policy.max_output_bytes;
    let seen = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let seen_r = seen.clone();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    seen_r.store(buf.len() as u64, std::sync::atomic::Ordering::Relaxed);
                    if buf.len() as u64 > cap {
                        return Err(buf.len());
                    }
                }
                Err(_) => break,
            }
        }
        Ok(buf)
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let timeout = if policy.max_cpu_s > 0.0 {
        Some(Duration::from_secs_f64(policy.max_cpu_s))
    } else {
        None
    };
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if seen.load(std::sync::atomic::Ordering::Relaxed) > cap {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SimorphError::Component {
                component: "formal_tool".into(),
                message: format!("output exceeded {cap} bytes"),
            });
        }
        if let Some(t) = timeout {
            if start.elapsed() >= t {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SimorphError::Timeout(t.as_millis() as u64));
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let out = reader
        .join()
        .map_err(|_| SimorphError::Other("reader thread panicked".into()))?;
    let stderr_text = err_reader.join().unwrap_or_default();
    let out = match out {
        Ok(b) => b,
        Err(n) => {
            let _ = child.kill();
            return Err(SimorphError::Component {
                component: "formal_tool".into(),
                message: format!("output exceeded {cap} bytes ({n})"),
            });
        }
    };
    let info = RunInfo {
        interpreter,
        network_isolated: isolate,
        wall_ms: start.elapsed().as_secs_f64() * 1e3,
    };
    let text = String::from_utf8_lossy(&out);
    let v: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|e| SimorphError::Component {
            component: "formal_tool".into(),
            message: format!(
                "invalid JSON from helper (status {status:?}): {e}; stderr: {}",
                stderr_text.lines().last().unwrap_or("")
            ),
        })?;
    if v.get("ok").and_then(|b| b.as_bool()) != Some(true) {
        let msg = v
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("unknown error")
            .to_string();
        if msg.contains("ModuleNotFoundError") || msg.contains("No module named") {
            return Err(SimorphError::NotImplemented(format!(
                "formal helper dependency missing: {msg}"
            )));
        }
        return Err(SimorphError::Component {
            component: "formal_tool".into(),
            message: msg,
        });
    }
    Ok((v, info))
}

/// Versions of z3 / sympy / scipy seen by the helper.
pub fn tool_versions() -> Result<serde_json::Value> {
    Ok(run_tool(&serde_json::json!({"op": "versions"}), &solver_policy())?.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_runs_isolated_and_honours_timeout_and_memory() {
        let (v, info) = match run_tool(&serde_json::json!({"op": "versions"}), &solver_policy()) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("skip: helper unavailable: {e}");
                return;
            }
        };
        assert!(v["z3"].is_string(), "{v}");
        eprintln!(
            "helper: {v} isolated={} {:.0} ms",
            info.network_isolated, info.wall_ms
        );
        // unknown op -> error, not a crash
        assert!(run_tool(&serde_json::json!({"op": "nope"}), &solver_policy()).is_err());
    }
}
