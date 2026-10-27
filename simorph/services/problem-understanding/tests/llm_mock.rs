//! The LLM proposer is tested only against a local mock server.  Its
//! proposals are re-validated: an invented port is rejected.

mod common;
use simorph_svc_problem_understanding::llm::*;
use simorph_svc_problem_understanding::{AmbiguityKind, Interpreter};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

/// Serve exactly one HTTP request with a canned body; returns the URL.
fn mock_server(body: String) -> (String, std::thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/messages", l.local_addr().unwrap());
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut len = 0usize;
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            if line.trim().is_empty() {
                break;
            }
            if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().unwrap();
            }
            headers.push_str(&line);
        }
        let mut req = vec![0u8; len];
        r.read_exact(&mut req).unwrap();
        let resp = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", body.len(), body);
        s.write_all(resp.as_bytes()).unwrap();
        headers + &String::from_utf8_lossy(&req)
    });
    (url, h)
}

#[test]
fn llm_proposal_is_revalidated_against_the_catalogue() {
    let proposal = serde_json::json!({
        "title": "Poutre",
        "knowns": [{"name": "load", "value": 1.0, "unit": "kN"}, {"name": "made_up_port", "value": 3.0, "unit": "m"}],
        "design_variables": [{"name": "length", "unit": "mm", "low": 1000.0, "high": 3000.0}],
        "objectives": [{"name": "flèche", "direction": "minimize", "variable": "tip_deflection", "unit": "m"}],
        "constraints": [{"name": "c", "variable": "tip_deflection", "kind": "upper_bound", "bound": 20.0, "unit": "s"}],
        "unknowns": []
    });
    let body = serde_json::json!({"content": [{"type": "text", "text": format!("Voici :\n```json\n{proposal}\n```")}]}).to_string();
    let (url, h) = mock_server(body);
    let cfg = LlmConfig {
        url,
        model: "mock-model".into(),
        api_key: "k-test".into(),
        style: ApiStyle::Anthropic,
        max_tokens: 512,
        timeout_s: 10,
    };
    let interp = LlmInterpreter {
        client: LlmClient::new(cfg),
    };
    let r = interp
        .interpret("Une poutre de 1 kN …", &common::manifests())
        .unwrap();
    let request = h.join().unwrap();
    assert!(
        request.contains("x-api-key: k-test") && request.contains("mock-model"),
        "{request}"
    );
    assert!(
        request.contains("tip_deflection"),
        "the prompt lists catalogue ports"
    );
    // valid items kept, invented port and wrong unit rejected
    assert!(r.spec.knowns.iter().any(|k| k.name == "load"));
    assert!(r.spec.knowns.iter().all(|k| k.name != "made_up_port"));
    assert!(
        r.spec.constraints.is_empty(),
        "constraint in seconds on a length must be rejected"
    );
    assert_eq!(
        r.ambiguities
            .iter()
            .filter(|a| a.kind == AmbiguityKind::InvalidProposal)
            .count(),
        2
    );
    assert!(r.mappings.iter().all(|m| m.source == "llm"));
}

#[test]
fn unconfigured_llm_is_not_implemented() {
    if std::env::var("SIMORPH_LLM_URL").is_ok() {
        return; // a real endpoint is configured on this machine
    }
    assert!(matches!(
        LlmInterpreter::from_env(),
        Err(simorph_core::SimorphError::NotImplemented(_))
    ));
}
