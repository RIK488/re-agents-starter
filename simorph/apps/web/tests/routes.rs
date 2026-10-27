//! One test per route (AC-UI-1).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use simorph_web::{build_app, AppState};
use std::sync::Arc;
use tower::ServiceExt;

fn app() -> (axum::Router, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("simorph-web-{}", simorph_core::RunId::new()));
    let state = Arc::new(AppState::open(dir.to_str().unwrap()).unwrap());
    (build_app(state), dir)
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(
            body.map(|b| Body::from(b.to_string()))
                .unwrap_or(Body::empty()),
        )
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}));
    (status, v)
}

async fn wait_job(app: &axum::Router, id: &str) -> Value {
    for _ in 0..600 {
        let (_, j) = call(app, "GET", &format!("/api/jobs/{id}"), None).await;
        if j["status"] != "running" {
            return j;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("job {id} did not finish");
}

#[tokio::test]
async fn every_route_is_wired_to_a_real_action() {
    let (app, dir) = app();
    let (s, v) = call(&app, "GET", "/health", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["ok"], true);
    let (s, v) = call(&app, "GET", "/", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["raw"].as_str().unwrap().contains("SIMORPH"));
    let (s, v) = call(&app, "GET", "/api/actions", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), simorph_cli::ACTIONS.len());
    // components
    let (s, v) = call(&app, "GET", "/api/components?domain=aerospace", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v.as_array().unwrap().len() >= 2);
    let (s, v) = call(
        &app,
        "GET",
        "/api/components/engineering.beam.cantilever",
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["name"], "engineering.beam.cantilever");
    let (s, _) = call(&app, "GET", "/api/components/nope", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // units
    let (s, v) = call(
        &app,
        "POST",
        "/api/units/convert",
        Some(json!({"value": 1.0, "from": "km", "to": "m"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["value"], 1000.0);
    let (s, _) = call(
        &app,
        "POST",
        "/api/units/convert",
        Some(json!({"value": 1.0, "from": "km", "to": "s"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, v) = call(
        &app,
        "POST",
        "/api/units/check",
        Some(json!({"a": "W", "b": "V*A"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["same_dimension"], true);
    // problems
    let (s, v) = call(&app, "GET", "/api/problems/examples", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v.as_array().unwrap().contains(&json!("beam")));
    let (s, beam) = call(&app, "GET", "/api/problems/examples/beam", None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = call(&app, "POST", "/api/problems/validate", Some(beam.clone())).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["valid"], true);
    let (s, v) = call(&app, "POST", "/api/analyse", Some(beam.clone())).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["markdown"].as_str().unwrap().contains("Analyse"));
    let (s, v) = call(&app, "POST", "/api/plan", Some(beam.clone())).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["programme"]["experiments"].as_array().unwrap().len() >= 3);
    // campaign job
    let (s, v) = call(&app, "POST", "/api/campaigns", Some(beam.clone())).await;
    assert_eq!(s, StatusCode::OK);
    let id = v["id"].as_str().unwrap().to_string();
    let j = wait_job(&app, &id).await;
    assert_eq!(j["status"], "done", "{j}");
    assert!(j["report_markdown"]
        .as_str()
        .unwrap()
        .contains("Conclusions"));
    let (s, ev) = call(&app, "GET", &format!("/api/campaigns/{id}/events"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!ev["provenance"].as_array().unwrap().is_empty());
    let (s, v) = call(&app, "GET", "/api/campaigns", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!v["archived"].as_array().unwrap().is_empty());
    let archived = v["archived"][0].as_str().unwrap().to_string();
    let (s, v) = call(&app, "GET", &format!("/api/campaigns/{archived}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["status"], "done");
    // cancel on an unknown job
    let (s, _) = call(&app, "POST", "/api/jobs/unknown/cancel", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // run + validation
    let (s, v) = call(&app, "POST", "/api/runs", Some(json!({"component": "operations.queue.mm1", "inputs": {"arrival_rate": 1.0, "service_rate": 2.0}}))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["result"]["outputs"]["mean_customers"], 1.0);
    assert_eq!(v["validation"]["verdict"], "computed");
    // experiments
    let (s, v) = call(&app, "GET", "/api/experiments", None).await;
    assert_eq!(s, StatusCode::OK);
    let eid = v[0].as_str().unwrap().to_string();
    let (s, v) = call(
        &app,
        "POST",
        &format!("/api/experiments/{eid}/reproduce"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["bitwise_identical"], true);
    // optimisation + uncertainty
    let (s, v) = call(
        &app,
        "POST",
        "/api/optimisation/recommend",
        Some(beam.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(!v["ranking"].as_array().unwrap().is_empty());
    let (s, v) = call(&app, "POST", "/api/uncertainty/sobol", Some(json!({"component": "engineering.beam.cantilever", "inputs": {"length": 2.0, "load": 1000.0, "youngs_modulus": 2.1e11, "second_moment": 8.33e-6}, "vary": {"length": 0.1, "load": 100.0}, "output": "tip_deflection", "n": 128}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["indices"].as_array().unwrap().len(), 2);
    // lab
    let (s, v) = call(&app, "GET", "/api/lab/suites", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 3);
    let (s, v) = call(
        &app,
        "POST",
        "/api/lab/run",
        Some(json!({"suite": "reasoner", "iterations": 1})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let j = wait_job(&app, v["id"].as_str().unwrap()).await;
    assert_eq!(j["status"], "done", "{j}");
    // provenance
    let (s, v) = call(&app, "GET", "/api/provenance?kind=validation&limit=5", None).await;
    assert_eq!(s, StatusCode::OK);
    let rid = v["records"][0]["id"].as_str().unwrap().to_string();
    let (s, v) = call(&app, "GET", &format!("/api/provenance/{rid}/explain"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["markdown"].as_str().is_some());
    let (s, v) = call(&app, "GET", "/api/provenance/verify", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["intact"], true);
    // model factory, simulator factory, knowledge
    let (s, v) = call(&app, "POST", "/api/models/assemble", Some(beam.clone())).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["markdown"].as_str().is_some());
    let (s, v) = call(&app, "POST", "/api/models/from-equations", Some(json!({"name": "decay", "equations": ["d(x)/dt = -k*x"], "variables": [{"name": "x", "unit": "mol"}], "parameters": [{"name": "k", "value": 0.5, "unit": "1/s"}, {"name": "x_0", "value": 2.0, "unit": "mol"}], "horizon": 2.0}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["trajectory"]["times"].as_array().unwrap().len() > 2);
    let (s, v) = call(&app, "POST", "/api/simulators/generate", Some(json!({"name": "decay-sim", "required_outputs": ["x"], "knowns": [{"name": "k", "value": 0.5, "unit": "1/s"}, {"name": "x_0", "value": 2.0, "unit": "mol"}], "horizon": 2.0, "equations": {"name": "decay", "equations": ["d(x)/dt = -k*x"], "variables": [{"name": "x", "unit": "mol"}], "parameters": [{"name": "k", "value": 0.5, "unit": "1/s"}, {"name": "x_0", "value": 2.0, "unit": "mol"}]}}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let j = wait_job(&app, v["id"].as_str().unwrap()).await;
    assert_eq!(j["status"], "done", "{j}");
    assert_eq!(j["result"]["steps"].as_array().unwrap().len(), 10);
    let (s, v) = call(&app, "GET", "/api/knowledge/memory", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        v["markdown"].as_str().unwrap().contains("tip_deflection")
            || v["markdown"].as_str().unwrap().len() > 50
    );
    let (s, v) = call(&app, "GET", "/api/knowledge/transfers?domain=energy", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v.is_array());
    // schemas, bench, metrics
    let (s, v) = call(&app, "GET", "/api/schemas", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        v.get("component-manifest.schema.json").is_some()
            || v.as_object()
                .unwrap()
                .keys()
                .any(|k| k.contains("manifest"))
    );
    let (s, v) = call(&app, "POST", "/api/bench", None).await;
    assert_eq!(s, StatusCode::OK);
    let j = wait_job(&app, v["id"].as_str().unwrap()).await;
    assert_eq!(j["status"], "done", "{j}");
    let (s, v) = call(&app, "GET", "/metrics", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["raw"].as_str().unwrap().contains("simorph_runs_total"));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn api_key_is_enforced_when_configured() {
    let dir = std::env::temp_dir().join(format!("simorph-web-key-{}", simorph_core::RunId::new()));
    let mut st = AppState::open(dir.to_str().unwrap()).unwrap();
    st.api_key = Some("secret".into());
    let app = build_app(Arc::new(st));
    let (s, _) = call(&app, "GET", "/api/components", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let req = Request::builder()
        .method("GET")
        .uri("/api/components")
        .header("x-api-key", "secret")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::OK
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Every action advertised by the registry is routed (no axum 404/405):
/// replaces the former comparison of two hand-written lists.
#[tokio::test]
async fn every_advertised_http_action_is_routed() {
    let (app, dir) = app();
    for a in simorph_cli::ACTIONS {
        let (method, path) = a.http.split_once(' ').unwrap();
        let path = path
            .split('?')
            .next()
            .unwrap()
            .replace("{name}", "x")
            .replace("{id}", "00000000-0000-0000-0000-000000000000");
        let req = Request::builder()
            .method(method)
            .uri(&path)
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        // An unrouted path gives an empty 404 or a 405; a routed one answers
        // with JSON (possibly an application error such as 400/404/422).
        assert_ne!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{} {}",
            method,
            path
        );
        assert!(
            !(status == StatusCode::NOT_FOUND && bytes.is_empty()),
            "action {} not routed: {} {}",
            a.id,
            method,
            path
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn viewer_keys_are_read_only_and_cors_is_closed_by_default() {
    let dir =
        std::env::temp_dir().join(format!("simorph-web-roles-{}", simorph_core::RunId::new()));
    let mut st = AppState::open(dir.to_str().unwrap()).unwrap();
    st.api_keys = vec![
        ("view".into(), simorph_security::Role::Viewer),
        ("res".into(), simorph_security::Role::Researcher),
    ];
    let app = build_app(Arc::new(st));
    let send = |key: &str, method: &str, uri: &str, body: Option<Value>| {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-api-key", key)
            .body(
                body.map(|b| Body::from(b.to_string()))
                    .unwrap_or(Body::empty()),
            )
            .unwrap()
    };
    let run = json!({"component": "operations.queue.mm1", "inputs": {"arrival_rate": 1.0, "service_rate": 2.0}});
    assert_eq!(
        app.clone()
            .oneshot(send("view", "GET", "/api/components", None))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.clone()
            .oneshot(send("view", "POST", "/api/runs", Some(run.clone())))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(send("res", "POST", "/api/runs", Some(run)))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.clone()
            .oneshot(send("bad", "GET", "/api/components", None))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    // A cross-origin request gets no Access-Control-Allow-Origin header.
    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .header("origin", "https://evil.example")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.headers().get("access-control-allow-origin").is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn natural_language_statement_is_understood_solved_and_replayed() {
    let (app, dir) = app();
    let text = "Une conduite d'eau d'une mine mesure 2 km de long. La vitesse d'écoulement est de 2,5 m/s, la masse volumique de 1000 kg/m³, la viscosité dynamique de 1 mPa·s et la rugosité de 0,05 mm. Choisir le diamètre entre 100 et 600 mm pour minimiser la perte de charge, qui ne doit pas dépasser 5 bar. Budget : 500 simulations.";
    let (s, v) = call(&app, "POST", "/api/understand", Some(json!({"text": text}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["spec"]["design_variables"][0]["unit"], "mm");
    let (s, v) = call(&app, "POST", "/api/agents/run", Some(json!({"text": text}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let j = wait_job(&app, v["id"].as_str().unwrap()).await;
    assert_eq!(j["status"], "done", "{j}");
    assert_eq!(j["result"]["validated"], true, "{j}");
    let trace = std::path::PathBuf::from(j["result"]["trace_path"].as_str().unwrap());
    let session = trace.parent().unwrap().file_name().unwrap().to_str().unwrap();
    let (s, v) = call(&app, "POST", "/api/agents/replay", Some(json!({"session": session}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["identical"], true, "{v}");
    // only session ids are accepted, never paths
    let (s, _) = call(&app, "POST", "/api/agents/replay", Some(json!({"session": "../../etc"}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn uploaded_data_is_inspected_and_names_never_become_paths() {
    let (app, dir) = app();
    let csv = "diameter [mm],pressure_drop [bar]\n200,1.5\n300,0.21\n";
    let (s, v) = call(&app, "POST", "/api/data/inspect", Some(json!({"filename": "../../obs.csv", "content": csv}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["schema"]["columns"][1]["unit"], "bar");
    assert!(v["provenance"]["uri"].as_str().unwrap().contains("uploads"));
    assert!(!dir.parent().unwrap().join("obs.csv").exists());
    let (s, _) = call(&app, "POST", "/api/data/inspect", Some(json!({"filename": "x.exe", "content": "1"}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "POST", "/api/fmu/inspect", Some(json!({"filename": "m.fmu", "content_base64": "bm90IGEgemlw"}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let _ = std::fs::remove_dir_all(dir);
}
