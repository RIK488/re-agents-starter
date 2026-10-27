//! Multi-process cluster tests: real `simorph-worker` processes over TCP.

use simorph_core::*;
use simorph_distributed::*;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SECRET: &str = "cluster-test-secret";

struct Workers(Vec<Child>);
impl Drop for Workers {
    fn drop(&mut self) {
        for c in &mut self.0 {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn spawn_worker(addr: &str, id: &str, slots: usize, secret: Option<&str>) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_simorph-worker"));
    cmd.args([
        "--connect",
        addr,
        "--id",
        id,
        "--slots",
        &slots.to_string(),
        "--heartbeat-ms",
        "100",
    ])
    .env_remove("SIMORPH_WORKER_SECRET")
    .stdout(Stdio::null())
    .stderr(Stdio::piped());
    if let Some(s) = secret {
        cmd.env("SIMORPH_WORKER_SECRET", s);
    }
    cmd.spawn().expect("spawn worker")
}

fn cluster(n: usize, slots: usize, cfg: CoordinatorConfig) -> (Coordinator, Workers) {
    let coord = Coordinator::bind(
        "127.0.0.1:0",
        CoordinatorConfig {
            secret: Some(SECRET.as_bytes().to_vec()),
            ..cfg
        },
    )
    .unwrap();
    let addr = coord.addr().to_string();
    let workers = Workers(
        (0..n)
            .map(|i| spawn_worker(&addr, &format!("w{i}"), slots, Some(SECRET)))
            .collect(),
    );
    coord.wait_for_workers(n, Duration::from_secs(30)).unwrap();
    (coord, workers)
}

fn beam_request(i: usize) -> TaskRequest {
    let id = ComponentId::deterministic("engineering.beam.cantilever@0.1.0");
    TaskRequest::new(id)
        .input("length", 1.0 + i as f64 * 0.01)
        .input("load", 1000.0 + i as f64)
        .input("youngs_modulus", 2.1e11)
        .input("second_moment", 8.33e-6)
}

#[test]
fn results_identical_to_local_execution_and_load_is_spread() {
    let (coord, _w) = cluster(3, 4, CoordinatorConfig::default());
    let local = simorph_plugin_engineering::beam::Cantilever::new();
    assert_eq!(beam_request(0).component, local.manifest().id);
    let ids: Vec<TaskId> = (0..200)
        .map(|i| {
            coord
                .submit(TaskSpec::new(
                    "engineering.beam.cantilever",
                    beam_request(i),
                ))
                .unwrap()
        })
        .collect();
    let outs = coord.wait_all(&ids, Duration::from_secs(60)).unwrap();
    for (i, o) in outs.iter().enumerate() {
        assert_eq!(o.kind, OutcomeKind::Done, "{o:?}");
        let remote = o.result.as_ref().unwrap();
        let here = run_component(&local, &beam_request(i), &ExecutionControl::default()).unwrap();
        assert_eq!(remote.output_hash(), here.output_hash(), "task {i}");
        assert_eq!(remote.request_hash, here.request_hash);
    }
    let m = coord.metrics();
    assert_eq!(m.completed, 200);
    assert!(m.workers.iter().all(|w| w.completed > 0), "{:?}", m.workers);
}

#[test]
fn killing_a_worker_mid_run_loses_no_task_and_records_each_once() {
    let (coord, mut w) = cluster(
        3,
        4,
        CoordinatorConfig {
            heartbeat_timeout: Duration::from_millis(800),
            ..Default::default()
        },
    );
    let ids: Vec<TaskId> = (0..200)
        .map(|i| {
            coord
                .submit(TaskSpec::new(
                    "test.sleep",
                    TaskRequest::new(ComponentId::new())
                        .input("x", i as f64)
                        .input("ms", 20.0),
                ))
                .unwrap()
        })
        .collect();
    // Wait until some work is done, then kill w1 while it holds running tasks.
    let t0 = Instant::now();
    while coord.metrics().completed < 40 && t0.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(5));
    }
    w.0[1].kill().unwrap();
    let outs = coord.wait_all(&ids, Duration::from_secs(60)).unwrap();
    let mut seen = std::collections::HashSet::new();
    for (i, o) in outs.iter().enumerate() {
        assert_eq!(o.kind, OutcomeKind::Done, "{o:?}");
        assert_eq!(
            o.result.as_ref().unwrap().get_f64("y").unwrap(),
            (i * i) as f64
        );
        assert!(seen.insert(o.task_id));
    }
    let m = coord.metrics();
    assert_eq!(m.completed, 200, "{m:?}");
    assert!(m.retries > 0, "the killed worker held running tasks: {m:?}");
    assert!(!m.workers.iter().find(|x| x.id == "w1").unwrap().alive);
    let retried = outs.iter().filter(|o| o.attempts > 1).count();
    eprintln!(
        "failover: {} tasks retried, retries={}, completed={}",
        retried, m.retries, m.completed
    );
}

#[test]
fn heartbeat_loss_requeues_and_late_results_are_not_double_counted() {
    let (coord, w) = cluster(
        2,
        2,
        CoordinatorConfig {
            heartbeat_timeout: Duration::from_millis(500),
            ..Default::default()
        },
    );
    let ids: Vec<TaskId> = (0..20)
        .map(|i| {
            coord
                .submit(TaskSpec::new(
                    "test.sleep",
                    TaskRequest::new(ComponentId::new())
                        .input("x", i as f64)
                        .input("ms", 150.0),
                ))
                .unwrap()
        })
        .collect();
    std::thread::sleep(Duration::from_millis(60));
    let pid = w.0[0].id().to_string();
    assert!(Command::new("kill")
        .args(["-STOP", &pid])
        .status()
        .unwrap()
        .success());
    std::thread::sleep(Duration::from_millis(1200));
    assert!(Command::new("kill")
        .args(["-CONT", &pid])
        .status()
        .unwrap()
        .success());
    let outs = coord.wait_all(&ids, Duration::from_secs(60)).unwrap();
    assert!(outs.iter().all(|o| o.kind == OutcomeKind::Done));
    let m = coord.metrics();
    assert_eq!(m.completed, 20, "{m:?}");
    assert!(
        !m.workers.iter().find(|x| x.id == "w0").unwrap().alive,
        "frozen worker must be declared dead"
    );
    assert!(m.retries > 0);
}

#[test]
fn unauthenticated_and_wrong_secret_workers_are_rejected() {
    let coord = Coordinator::bind(
        "127.0.0.1:0",
        CoordinatorConfig {
            secret: Some(SECRET.as_bytes().to_vec()),
            ..Default::default()
        },
    )
    .unwrap();
    let addr = coord.addr().to_string();
    let mut none = spawn_worker(&addr, "intruder", 1, None);
    let mut wrong = spawn_worker(&addr, "guesser", 1, Some("not-the-secret"));
    let s1 = none.wait().unwrap();
    let s2 = wrong.wait().unwrap();
    assert_eq!(s1.code(), Some(3));
    assert_eq!(s2.code(), Some(3));
    assert_eq!(coord.live_workers(), 0);
    let m = coord.metrics();
    assert_eq!(m.rejected_workers.len(), 2, "{:?}", m.rejected_workers);
    assert!(m
        .rejected_workers
        .iter()
        .all(|r| r.contains("authentication failed")));
}

#[test]
fn gpu_task_without_gpu_worker_is_unschedulable_not_run_on_cpu() {
    let (coord, _w) = cluster(
        1,
        2,
        CoordinatorConfig {
            unschedulable_grace: Duration::from_millis(300),
            ..Default::default()
        },
    );
    let inv = &coord.metrics().workers[0];
    let req = ResourceRequirements {
        gpu_count: 1,
        ..Default::default()
    };
    let id = coord
        .submit(
            TaskSpec::new(
                "test.square",
                TaskRequest::new(ComponentId::new()).input("x", 3.0),
            )
            .requirements(req),
        )
        .unwrap();
    let o = coord.wait(id, Duration::from_secs(10)).unwrap();
    if inv.gpus == 0 {
        assert_eq!(o.kind, OutcomeKind::Unschedulable, "{o:?}");
        assert!(o.error.unwrap().contains("1 GPU"));
        assert_eq!(o.attempts, 0, "must never have been dispatched");
    } else {
        assert_eq!(o.kind, OutcomeKind::Done);
    }
    // an unknown component is unschedulable too
    let id = coord
        .submit(TaskSpec::new("nope", TaskRequest::new(ComponentId::new())))
        .unwrap();
    assert_eq!(
        coord.wait(id, Duration::from_secs(10)).unwrap().kind,
        OutcomeKind::Unschedulable
    );
}

#[test]
fn cancellation_timeout_dedup_priority_and_back_pressure() {
    let (coord, _w) = cluster(1, 1, CoordinatorConfig::default());
    // cancellation of a running task
    let long = coord
        .submit(TaskSpec::new(
            "test.sleep",
            TaskRequest::new(ComponentId::new())
                .input("x", 1.0)
                .input("ms", 10_000.0),
        ))
        .unwrap();
    let t0 = Instant::now();
    while coord.status(long).unwrap().kind != OutcomeKind::Running {
        std::thread::sleep(Duration::from_millis(5));
        assert!(t0.elapsed() < Duration::from_secs(10));
    }
    coord.cancel(long).unwrap();
    assert_eq!(
        coord.wait(long, Duration::from_secs(1)).unwrap().kind,
        OutcomeKind::Cancelled
    );
    // the single slot is free again: next task completes promptly
    let quick = coord
        .submit(TaskSpec::new(
            "test.square",
            TaskRequest::new(ComponentId::new()).input("x", 4.0),
        ))
        .unwrap();
    let o = coord.wait(quick, Duration::from_secs(5)).unwrap();
    assert_eq!(o.result.unwrap().get_f64("y").unwrap(), 16.0);
    // timeout with retries exhausted
    let slow = coord
        .submit(
            TaskSpec::new(
                "test.sleep",
                TaskRequest::new(ComponentId::new())
                    .input("x", 2.0)
                    .input("ms", 5_000.0),
            )
            .timeout(Duration::from_millis(150))
            .max_attempts(2),
        )
        .unwrap();
    let o = coord.wait(slow, Duration::from_secs(10)).unwrap();
    assert_eq!(o.kind, OutcomeKind::Failed);
    assert_eq!(o.attempts, 2);
    assert!(o.error.unwrap().contains("timeout"));
    // de-duplication: same request hash → same task
    let req = TaskRequest::new(ComponentId::new()).input("x", 7.0);
    let a = coord
        .submit(TaskSpec::new("test.square", req.clone()))
        .unwrap();
    let b = coord
        .submit(TaskSpec::new(
            "test.square",
            TaskRequest {
                run_id: RunId::new(),
                ..req
            },
        ))
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(coord.metrics().dedup_hits, 1);
    coord.wait(a, Duration::from_secs(5)).unwrap();

    // priority: with the slot busy, a high-priority task overtakes a queue of low ones
    let blocker = coord
        .submit(TaskSpec::new(
            "test.sleep",
            TaskRequest::new(ComponentId::new())
                .input("x", 3.0)
                .input("ms", 200.0),
        ))
        .unwrap();
    while coord.status(blocker).unwrap().kind != OutcomeKind::Running {
        std::thread::sleep(Duration::from_millis(2));
    }
    let lows: Vec<TaskId> = (0..5)
        .map(|i| {
            coord
                .submit(TaskSpec::new(
                    "test.pid",
                    TaskRequest::new(ComponentId::new()).input("i", i as f64),
                ))
                .unwrap()
        })
        .collect();
    let high = coord
        .submit(
            TaskSpec::new(
                "test.pid",
                TaskRequest::new(ComponentId::new()).input("i", 99.0),
            )
            .priority(10),
        )
        .unwrap();
    let lows = coord.wait_all(&lows, Duration::from_secs(10)).unwrap();
    let h = coord.wait(high, Duration::from_secs(5)).unwrap();
    assert_eq!(h.kind, OutcomeKind::Done);
    let hi = h.completion_index.unwrap();
    assert!(
        lows.iter().all(|l| l.completion_index.unwrap() > hi),
        "high-priority task must overtake the queued low-priority ones"
    );

    // back-pressure: coordinator without workers and a small queue
    let lonely = Coordinator::bind(
        "127.0.0.1:0",
        CoordinatorConfig {
            max_queue: 3,
            unschedulable_grace: Duration::from_secs(60),
            ..Default::default()
        },
    )
    .unwrap();
    for i in 0..3 {
        lonely
            .submit(TaskSpec::new(
                "test.square",
                TaskRequest::new(ComponentId::new()).input("x", i as f64),
            ))
            .unwrap();
    }
    let e = lonely
        .submit(TaskSpec::new(
            "test.square",
            TaskRequest::new(ComponentId::new()).input("x", 9.0),
        ))
        .unwrap_err();
    assert!(e.to_string().contains("back-pressure"), "{e}");
}

/// Throughput and per-task overhead versus in-process execution.
#[test]
fn throughput_and_overhead_measurement() {
    let (coord, _w) = cluster(3, 4, CoordinatorConfig::default());
    let n = 600;
    // in-process baseline
    let local = simorph_plugin_engineering::beam::Cantilever::new();
    let t0 = Instant::now();
    for i in 0..n {
        run_component(&local, &beam_request(i), &ExecutionControl::default()).unwrap();
    }
    let local_s = t0.elapsed().as_secs_f64();
    let t0 = Instant::now();
    let ids: Vec<TaskId> = (0..n)
        .map(|i| {
            coord
                .submit(TaskSpec::new(
                    "engineering.beam.cantilever",
                    beam_request(10_000 + i),
                ))
                .unwrap()
        })
        .collect();
    coord.wait_all(&ids, Duration::from_secs(120)).unwrap();
    let dist_s = t0.elapsed().as_secs_f64();
    // latency of a single round trip
    let t1 = Instant::now();
    let one = coord
        .submit(TaskSpec::new(
            "test.square",
            TaskRequest::new(ComponentId::new()).input("x", 1.5),
        ))
        .unwrap();
    coord.wait(one, Duration::from_secs(5)).unwrap();
    let rtt_ms = t1.elapsed().as_secs_f64() * 1e3;
    let m = coord.metrics();
    eprintln!(
        "THROUGHPUT n={n}: in-process {:.0} tasks/s ({:.1} µs/task); distributed 3 workers×4 slots {:.0} tasks/s ({:.0} µs/task); single round trip {:.2} ms; utilisation {:?}",
        n as f64 / local_s,
        local_s / n as f64 * 1e6,
        n as f64 / dist_s,
        dist_s / n as f64 * 1e6,
        rtt_ms,
        m.workers.iter().map(|w| format!("{}:{:.3}", w.id, w.utilisation)).collect::<Vec<_>>()
    );
    assert!(
        n as f64 / dist_s > 50.0,
        "distributed throughput unexpectedly low"
    );
}
