//! Program evolution: sandbox safety, evolution gain on held-out instances,
//! and the LLM mutator against a local mock endpoint.

use simorph_core::*;
use simorph_evolution::binpacking::*;
use simorph_evolution::program::*;
use simorph_provenance::{Ledger, ProvenanceKind};
use std::io::{BufRead, BufReader, Read, Write};

fn task() -> BinPackingTask {
    BinPackingTask::new(10, 10, 8, 5)
}

#[test]
fn malicious_or_broken_candidates_score_as_failures_and_cannot_touch_the_evaluator() {
    let t = task();
    let sb = PythonSandbox::new("pack", 2.0).unwrap();
    let ctl = ExecutionControl::default();
    let marker = std::env::temp_dir().join(format!("simorph-evo-marker-{}", RunId::new()));
    let attacks = vec![
        ("import", "import os\ndef pack(items, capacity):\n    os.system('id')\n    return [0]*len(items)\n".to_string()),
        ("open", format!("def pack(items, capacity):\n    open('{}', 'w').write('x')\n    return list(range(len(items)))\n", marker.display())),
        ("dunder escape", "def pack(items, capacity):\n    s = ().__class__.__bases__[0].__subclasses__()\n    return list(range(len(items)))\n".to_string()),
        ("eval", "def pack(items, capacity):\n    return eval('list(range(len(items)))')\n".to_string()),
        ("infinite loop", "def pack(items, capacity):\n    while True:\n        pass\n".to_string()),
        ("syntax error", "def pack(items, capacity)\n    return []\n".to_string()),
        ("overload cheat", "def pack(items, capacity):\n    return [0]*len(items)\n".to_string()),
    ];
    for (name, src) in attacks {
        let out = sb.run(&src, &t.train().instances, &ctl);
        let s = t.score(t.train(), &out);
        println!(
            "{name:<15} failures {}/{} score {:.2} | {}",
            s.failures,
            s.per_instance.len(),
            s.score,
            s.messages.first().cloned().unwrap_or_default()
        );
        assert_eq!(s.failures, s.per_instance.len(), "{name} was not rejected");
        assert_eq!(s.score, INFEASIBLE_RATIO);
        t.train().verify().unwrap();
    }
    assert!(!marker.exists(), "candidate wrote a file");
    // the sandbox is still usable after a timeout
    let ok = sb.run(SEED_FIRST_FIT, &t.train().instances, &ctl);
    assert_eq!(t.score(t.train(), &ok).failures, 0);
}

#[test]
fn network_namespace_prefix_blocks_loopback_of_the_host() {
    let (prefix, isolated) = network_isolation_prefix();
    if !isolated {
        println!("network namespaces unavailable here: layer not active (reported by PythonSandbox::network_isolated)");
        return;
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let code = format!("import socket; socket.create_connection(('127.0.0.1', {port}), timeout=2); print('connected')");
    let py = simorph_adapter_python_bridge::python_interpreter();
    // control: without the prefix the connection succeeds
    let ctrl = std::process::Command::new(&py)
        .args(["-I", "-c", &code])
        .output()
        .unwrap();
    assert!(
        ctrl.status.success(),
        "control connection failed: {}",
        String::from_utf8_lossy(&ctrl.stderr)
    );
    let mut cmd = std::process::Command::new(&prefix[0]);
    cmd.args(&prefix[1..]).arg(&py).args(["-I", "-c", &code]);
    let iso = cmd.output().unwrap();
    assert!(
        !iso.status.success(),
        "isolated child reached the host loopback"
    );
}

#[test]
fn evolution_improves_held_out_packing_over_the_seed() {
    let t = task();
    let sb = PythonSandbox::new("pack", 5.0).unwrap();
    let m = SyntacticMutator::new(11, snippets());
    let ledger = Ledger::in_memory();
    let cfg = ProgramEvolutionConfig {
        generations: 30,
        parents_per_generation: 4,
        seed: 3,
        ..Default::default()
    };
    let r = evolve_programs(
        &t,
        BinPackingTask::seed_program(),
        &[&m],
        &sb,
        &cfg,
        &ledger,
        &ExecutionControl::default(),
    )
    .unwrap();
    println!(
        "program evolution: seed held-out {:.4} → best held-out {:.4} (train {:.4} → {:.4}); {} candidates, {} failing; {:.1} s; isolated network {}\nbest origin: {}\n{}",
        r.seed_held_out.score, r.best_held_out.score, r.seed_train.score, r.best.train.score, r.evaluated, r.failed_candidates, r.wall_time_s, r.network_isolated, r.best.program.origin, r.best.program.source
    );
    assert!(
        r.best_held_out.score < r.seed_held_out.score - 0.02,
        "no held-out improvement"
    );
    assert_eq!(r.best_held_out.failures, 0);
    assert!(
        r.failed_candidates > 0,
        "the search should have produced (and discarded) broken candidates"
    );
    ledger.verify_chain().unwrap();
    assert_eq!(ledger.by_kind(ProvenanceKind::Run).len(), cfg.generations);
}

/// Minimal HTTP server answering one request with `body`; returns the raw request.
fn mock_server(body: String) -> (String, std::thread::JoinHandle<String>) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/v1/messages",
        l.local_addr().unwrap().port()
    );
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut head = String::new();
        let mut len = 0usize;
        loop {
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            if line.to_lowercase().starts_with("content-length:") {
                len = line[15..].trim().parse().unwrap();
            }
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let mut b = vec![0u8; len];
        r.read_exact(&mut b).unwrap();
        let resp = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", body.len(), body);
        s.write_all(resp.as_bytes()).unwrap();
        head + &String::from_utf8_lossy(&b)
    });
    (url, h)
}

#[test]
fn llm_mutator_against_local_mock_endpoint() {
    let improved = SEED_FIRST_FIT.replace(
        "    order = list(range(len(items)))\n",
        "    order = list(range(len(items)))\n    order.sort(key=lambda j: -items[j])\n",
    );
    let reply = serde_json::json!({"content": [{"type": "text", "text": format!("Sorting first helps.\n```python\n{improved}```")}]}).to_string();
    let (url, h) = mock_server(reply);
    let cfg = LlmConfig {
        url,
        model: "mock-model".into(),
        api_key: Some("test-key".into()),
        style: LlmStyle::Anthropic,
        max_tokens: 512,
        timeout_s: 10,
    };
    let t = task();
    let m = LlmMutator::new(cfg, "pack", t.description());
    let parent = BinPackingTask::seed_program();
    let kids = m
        .propose(
            &parent,
            &EvalFeedback {
                score: 1.2,
                failures: 0,
                messages: vec![],
            },
        )
        .unwrap();
    let request = h.join().unwrap();
    assert!(request.contains("x-api-key: test-key"), "{request}");
    assert!(request.contains("anthropic-version"));
    assert!(request.contains("mock-model") && request.contains("def pack(items, capacity)"));
    assert_eq!(kids.len(), 1);
    // the proposal is evaluated in the sandbox like any candidate
    let sb = PythonSandbox::new("pack", 5.0).unwrap();
    let ctl = ExecutionControl::default();
    let seed = t.score(
        t.held_out(),
        &sb.run(&parent.source, &t.held_out().instances, &ctl),
    );
    let child = t.score(
        t.held_out(),
        &sb.run(&kids[0].source, &t.held_out().instances, &ctl),
    );
    println!(
        "LLM (mock) proposal: held-out {:.4} vs seed {:.4}",
        child.score, seed.score
    );
    assert!(child.score < seed.score);
}
