//! Operating-system isolation applied to sandboxed child processes (Linux).
//!
//! Applied between `fork` and `exec` (async-signal-safe calls only):
//!
//! | Policy field          | Mechanism                                   |
//! |-----------------------|---------------------------------------------|
//! | `network == false`    | new empty network namespace (`unshare(CLONE_NEWNET)`, falling back to a user + network namespace when unprivileged); only a down loopback exists, so no TCP/UDP egress and no localhost access |
//! | `max_memory_mb`       | `RLIMIT_DATA` (heap, anonymous mappings, thread stacks) |
//! | `max_process_cpu_s`   | `RLIMIT_CPU` (SIGXCPU then SIGKILL)          |
//! | `max_file_bytes`      | `RLIMIT_FSIZE` (writes beyond fail with EFBIG) |
//! | always                | `RLIMIT_CORE = 0`, own process group (the whole group is killed on timeout) |
//!
//! When `require_os_isolation` is set, any failure aborts the spawn: the
//! component never runs with weaker isolation than requested.  Not covered
//! here (container layer): filesystem mount restrictions and process-count
//! limits.

use simorph_security::SandboxPolicy;
use std::io;
use std::process::Command;

/// Limits computed from a policy, copied into the child before `exec`.
#[derive(Debug, Clone, Copy)]
pub struct OsLimits {
    pub isolate_network: bool,
    pub data_bytes: Option<u64>,
    pub cpu_seconds: Option<u64>,
    pub file_bytes: Option<u64>,
    pub required: bool,
}

impl OsLimits {
    pub fn from_policy(p: &SandboxPolicy) -> Self {
        Self {
            isolate_network: !p.network,
            data_bytes: (p.max_memory_mb > 0).then(|| p.max_memory_mb.saturating_mul(1024 * 1024)),
            cpu_seconds: (p.max_process_cpu_s > 0.0).then(|| p.max_process_cpu_s.ceil() as u64),
            file_bytes: (p.max_file_bytes > 0).then_some(p.max_file_bytes),
            required: p.require_os_isolation,
        }
    }
}

#[cfg(target_os = "linux")]
fn set_limit(resource: libc::__rlimit_resource_t, value: u64) -> io::Result<()> {
    let lim = libc::rlimit {
        rlim_cur: value as libc::rlim_t,
        rlim_max: value as libc::rlim_t,
    };
    if unsafe { libc::setrlimit(resource, &lim) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_in_child(l: OsLimits) -> io::Result<()> {
    // Own process group so that timeouts kill grandchildren too.
    unsafe {
        libc::setpgid(0, 0);
    }
    let mut first_err: Option<io::Error> = None;
    let mut record = |r: io::Result<()>| {
        if let Err(e) = r {
            if first_err.is_none() {
                first_err = Some(e);
            }
        }
    };
    record(set_limit(libc::RLIMIT_CORE, 0));
    if let Some(b) = l.data_bytes {
        record(set_limit(libc::RLIMIT_DATA, b));
    }
    if let Some(s) = l.cpu_seconds {
        // soft = s (SIGXCPU), hard = s + 1 (SIGKILL)
        let lim = libc::rlimit {
            rlim_cur: s as libc::rlim_t,
            rlim_max: (s + 1) as libc::rlim_t,
        };
        if unsafe { libc::setrlimit(libc::RLIMIT_CPU, &lim) } != 0 {
            record(Err(io::Error::last_os_error()));
        }
    }
    if let Some(b) = l.file_bytes {
        record(set_limit(libc::RLIMIT_FSIZE, b));
    }
    if l.isolate_network {
        let ok = unsafe { libc::unshare(libc::CLONE_NEWNET) } == 0
            || unsafe { libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNET) } == 0;
        if !ok {
            record(Err(io::Error::last_os_error()));
        }
    }
    match first_err {
        Some(e) if l.required => Err(e),
        _ => Ok(()),
    }
}

/// Install the isolation hook on a command.
#[cfg(target_os = "linux")]
pub fn apply(cmd: &mut Command, limits: OsLimits) {
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(move || apply_in_child(limits));
    }
}

#[cfg(not(target_os = "linux"))]
pub fn apply(_cmd: &mut Command, limits: OsLimits) {
    // Only reachable on non-Linux targets; spawning then fails closed below.
    let _ = limits;
}

/// Whether this platform enforces the limits (used to fail closed).
pub fn supported() -> bool {
    cfg!(target_os = "linux")
}

/// Kill the whole process group of `pid` (created by [`apply`]).
#[cfg(unix)]
pub fn kill_group(pid: u32) {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

#[cfg(not(unix))]
pub fn kill_group(_pid: u32) {}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::process::Stdio;

    fn run(policy: &SandboxPolicy, script: &str) -> (i32, String) {
        let mut cmd = Command::new("python3");
        cmd.args(["-I", "-c", script])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .envs(policy.to_subprocess_env());
        apply(&mut cmd, OsLimits::from_policy(policy));
        let out = cmd.output().expect("spawn");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr),
        )
    }

    #[test]
    fn network_namespace_has_no_route_even_to_localhost() {
        // A listener on the host loopback is unreachable from the child.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let script = format!("import socket\ntry:\n    socket.create_connection(('127.0.0.1', {port}), timeout=2)\n    print('CONNECTED')\nexcept OSError as e:\n    print('BLOCKED', e.errno)\n");
        let (_, out) = run(&SandboxPolicy::strict(), &script);
        assert!(out.contains("BLOCKED"), "{out}");
        let (_, out) = run(&SandboxPolicy::strict().with_network(), &script);
        assert!(
            out.contains("CONNECTED"),
            "control with network allowed: {out}"
        );
    }

    #[test]
    fn memory_limit_stops_large_allocation() {
        let script = "try:\n    b = bytearray(512 * 1024 * 1024)\n    print('ALLOCATED')\nexcept MemoryError:\n    print('MEMORYERROR')\n";
        let (_, out) = run(&SandboxPolicy::strict().with_memory_mb(256), script);
        assert!(out.contains("MEMORYERROR"), "{out}");
        let (_, out) = run(&SandboxPolicy::strict().with_memory_mb(2048), script);
        assert!(out.contains("ALLOCATED"), "control: {out}");
    }

    #[test]
    fn cpu_budget_terminates_busy_loop() {
        let t0 = std::time::Instant::now();
        let (code, _) = run(
            &SandboxPolicy::strict().with_process_cpu_s(1.0),
            "while True:\n    pass\n",
        );
        assert_ne!(code, 0);
        assert!(t0.elapsed().as_secs_f64() < 10.0);
    }

    #[test]
    fn file_size_limit_is_enforced() {
        let dir = std::env::temp_dir().join(format!("simorph-fsize-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("big.bin");
        let script = format!("import signal\nsignal.signal(signal.SIGXFSZ, signal.SIG_IGN)\ntry:\n    open({:?}, 'wb').write(b'x' * 4_000_000)\n    print('WROTE')\nexcept OSError as e:\n    print('EFBIG', e.errno)\n", f.to_string_lossy());
        let (_, out) = run(
            &SandboxPolicy::strict().with_max_file_bytes(1_000_000),
            &script,
        );
        assert!(out.contains("EFBIG"), "{out}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
