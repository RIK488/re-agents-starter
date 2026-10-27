//! # simorph-web
//!
//! Axum router exposing the action library (`simorph_cli`) over HTTP, plus
//! the single-page interface in `static/index.html`.  Every route is backed
//! by a function of the action library and covered by a test in
//! `tests/routes.rs` (AC-UI-1).  Long operations (campaigns, lab loops,
//! benchmarks) run on the blocking pool and can be cancelled through the
//! cooperative cancellation token.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use indexmap::IndexMap;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use simorph_cli::*;
use simorph_core::*;
use simorph_telemetry::Metrics;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Job {
    pub id: String,
    pub kind: String,
    pub status: JobStatus,
    pub control: ExecutionControl,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Ledger sequence at job start (for live event polling).
    pub ledger_start: usize,
    pub result: Option<serde_json::Value>,
    pub report: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Running,
    Done,
    Failed,
    Cancelled,
}

pub struct AppState {
    pub workspace: Workspace,
    pub jobs: Mutex<HashMap<String, Job>>,
    pub metrics: Metrics,
    /// Legacy single key (`SIMORPH_API_KEY`), mapped to the Researcher role.
    pub api_key: Option<String>,
    /// Keys with roles (`SIMORPH_API_KEYS="key1:researcher,key2:viewer"`).
    pub api_keys: Vec<(String, simorph_security::Role)>,
    /// Origins allowed by CORS (`SIMORPH_CORS_ORIGINS`); empty = same-origin only.
    pub cors_origins: Vec<String>,
}

/// Maximum number of finished jobs kept in memory (oldest evicted first).
pub const MAX_FINISHED_JOBS: usize = 200;

fn parse_role(s: &str) -> Option<simorph_security::Role> {
    use simorph_security::Role::*;
    Some(match s.trim().to_lowercase().as_str() {
        "admin" => Admin,
        "researcher" => Researcher,
        "operator" => Operator,
        "viewer" => Viewer,
        "agent" => Agent,
        _ => return None,
    })
}

fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

impl AppState {
    pub fn open(data: &str) -> Result<Self> {
        let api_keys = std::env::var("SIMORPH_API_KEYS")
            .ok()
            .map(|v| {
                v.split(',')
                    .filter_map(|kv| kv.split_once(':'))
                    .filter_map(|(k, r)| parse_role(r).map(|role| (k.trim().to_string(), role)))
                    .filter(|(k, _)| !k.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let cors_origins = std::env::var("SIMORPH_CORS_ORIGINS")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            api_keys,
            cors_origins,
            workspace: Workspace::open(data)?,
            jobs: Mutex::new(HashMap::new()),
            metrics: Metrics::new(),
            api_key: std::env::var("SIMORPH_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
        })
    }
}

type Shared = Arc<AppState>;

/// API error → JSON {error}.
pub struct ApiError(StatusCode, String);

impl From<SimorphError> for ApiError {
    fn from(e: SimorphError) -> Self {
        let code = match &e {
            SimorphError::NotFound(_) => StatusCode::NOT_FOUND,
            SimorphError::InvalidInput(_)
            | SimorphError::Unit(_)
            | SimorphError::Dimensional(_)
            | SimorphError::Serialization(_) => StatusCode::BAD_REQUEST,
            SimorphError::PermissionDenied(_) | SimorphError::Protected(_) => StatusCode::FORBIDDEN,
            SimorphError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError(code, e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

type ApiResult<T> = std::result::Result<Json<T>, ApiError>;

/// Resolve the caller's role.  Without any configured key the server runs in
/// local mode (intended for a loopback bind) and every caller is a Researcher.
fn caller_role(
    state: &AppState,
    headers: &HeaderMap,
) -> std::result::Result<simorph_security::Role, ApiError> {
    if state.api_key.is_none() && state.api_keys.is_empty() {
        return Ok(simorph_security::Role::Researcher);
    }
    let given = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Some(k) = &state.api_key {
        if ct_eq(given, k) {
            return Ok(simorph_security::Role::Researcher);
        }
    }
    for (k, role) in &state.api_keys {
        if ct_eq(given, k) {
            return Ok(*role);
        }
    }
    Err(ApiError(
        StatusCode::UNAUTHORIZED,
        "clé API invalide".into(),
    ))
}

/// Read access: any authenticated role.
fn check_key(state: &AppState, headers: &HeaderMap) -> std::result::Result<(), ApiError> {
    caller_role(state, headers).map(|_| ())
}

/// Actions that execute components, generated code or long jobs require a
/// role holding `SpawnProcess` (Admin, Researcher, Operator); Viewer and
/// Agent keys are read-only on the HTTP API.
fn check_write(state: &AppState, headers: &HeaderMap) -> std::result::Result<(), ApiError> {
    use simorph_security::Role::*;
    match caller_role(state, headers)? {
        Admin | Researcher | Operator => Ok(()),
        r => Err(ApiError(
            StatusCode::FORBIDDEN,
            format!("le rôle {r:?} ne permet pas d'exécuter des calculs"),
        )),
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> std::result::Result<T, ApiError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(ApiError::from)
}

// ------------------------------------------------------------------ handlers

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"ok": true, "service": "simorph-web", "time": chrono::Utc::now()}))
}

async fn metrics(State(s): State<Shared>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        s.metrics.prometheus(),
    )
}

async fn actions() -> Json<Vec<ActionInfo>> {
    Json(ACTIONS.to_vec())
}

#[derive(Deserialize)]
struct ComponentsQuery {
    domain: Option<String>,
    capability: Option<String>,
}

async fn components_list(
    State(s): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<ComponentsQuery>,
) -> ApiResult<Vec<ComponentManifest>> {
    check_key(&s, &headers)?;
    Ok(Json(
        blocking(move || components(q.domain.as_deref(), q.capability.as_deref())).await?,
    ))
}

async fn component_show(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> ApiResult<ComponentManifest> {
    check_key(&s, &headers)?;
    Ok(Json(blocking(move || component(&name)).await?))
}

#[derive(Deserialize)]
struct ConvertReq {
    value: f64,
    from: String,
    to: String,
}

async fn units_convert(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<ConvertReq>,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    let v = simorph_units::convert(r.value, &r.from, &r.to)?;
    Ok(Json(serde_json::json!({"value": v, "unit": r.to})))
}

#[derive(Deserialize)]
struct CheckReq {
    a: String,
    b: String,
}

async fn units_check(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<CheckReq>,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    let ua = simorph_units::parse_unit(&r.a)?;
    let ub = simorph_units::parse_unit(&r.b)?;
    Ok(Json(
        serde_json::json!({"same_dimension": ua.dimension == ub.dimension, "dimension_a": ua.dimension.to_string(), "dimension_b": ub.dimension.to_string()}),
    ))
}

async fn examples_list() -> Json<Vec<&'static str>> {
    Json(examples::names())
}

async fn example_get(Path(name): Path<String>) -> ApiResult<ProblemSpec> {
    examples::example(&name)
        .map(Json)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("example `{name}`")))
}

async fn problem_validate(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(p): Json<ProblemSpec>,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    p.validate_structure()?;
    let mut unit_errors = vec![];
    for q in p
        .knowns
        .iter()
        .map(|q| (&q.name, &q.unit))
        .chain(p.unknowns.iter().map(|u| (&u.name, &u.unit)))
        .chain(p.design_variables.iter().map(|d| (&d.name, &d.unit)))
    {
        if let Err(e) = simorph_units::parse_unit(q.1) {
            unit_errors.push(format!("{} : {e}", q.0));
        }
    }
    Ok(Json(
        serde_json::json!({"valid": unit_errors.is_empty(), "unit_errors": unit_errors, "knowns": p.knowns.len(), "unknowns": p.unknowns.len(), "design_variables": p.design_variables.len(), "objectives": p.objectives.len(), "constraints": p.constraints.len()}),
    ))
}

async fn analyse_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(p): Json<ProblemSpec>,
) -> ApiResult<AnalysisOutput> {
    check_key(&s, &headers)?;
    Ok(Json(blocking(move || analyse(&p)).await?))
}

async fn plan_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(p): Json<ProblemSpec>,
) -> ApiResult<PlanOutput> {
    check_key(&s, &headers)?;
    Ok(Json(blocking(move || plan(&p)).await?))
}

#[derive(Serialize)]
pub struct JobView {
    pub id: String,
    pub kind: String,
    pub status: JobStatus,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub report_markdown: Option<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

fn job_view(j: &Job) -> JobView {
    JobView {
        id: j.id.clone(),
        kind: j.kind.clone(),
        status: j.status,
        started_at: j.started_at,
        report_markdown: j.report.clone(),
        result: j.result.clone(),
        error: j.error.clone(),
    }
}

fn start_job(
    state: &Shared,
    kind: &str,
    f: impl FnOnce(&Workspace, &ExecutionControl) -> Result<(serde_json::Value, String)>
        + Send
        + 'static,
) -> String {
    let id = RunId::new().to_string();
    let control = ExecutionControl::default();
    {
        // Bound memory: evict the oldest finished jobs beyond the limit.
        let mut jobs = state.jobs.lock();
        let mut finished: Vec<(chrono::DateTime<chrono::Utc>, String)> = jobs
            .values()
            .filter(|j| j.status != JobStatus::Running)
            .map(|j| (j.started_at, j.id.clone()))
            .collect();
        if finished.len() >= MAX_FINISHED_JOBS {
            finished.sort();
            for (_, old) in finished.iter().take(finished.len() + 1 - MAX_FINISHED_JOBS) {
                jobs.remove(old);
            }
        }
    }
    state.jobs.lock().insert(
        id.clone(),
        Job {
            id: id.clone(),
            kind: kind.into(),
            status: JobStatus::Running,
            control: control.clone(),
            started_at: chrono::Utc::now(),
            ledger_start: state.workspace.ledger.len(),
            result: None,
            report: None,
            error: None,
        },
    );
    let st = state.clone();
    let jid = id.clone();
    let kind = kind.to_string();
    tokio::task::spawn_blocking(move || {
        let outcome = f(&st.workspace, &control);
        let mut jobs = st.jobs.lock();
        if let Some(j) = jobs.get_mut(&jid) {
            match outcome {
                Ok((result, report)) => {
                    j.status = JobStatus::Done;
                    j.result = Some(result);
                    j.report = Some(report);
                }
                Err(SimorphError::Cancelled) => {
                    j.status = JobStatus::Cancelled;
                    j.error = Some("annulé".into());
                }
                Err(e) => {
                    j.status = JobStatus::Failed;
                    j.error = Some(e.to_string());
                }
            }
        }
        st.metrics.inc(&format!("simorph_jobs_{kind}_total"));
    });
    id
}

async fn campaign_start(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(p): Json<ProblemSpec>,
) -> ApiResult<serde_json::Value> {
    check_write(&s, &headers)?;
    p.validate_structure()?;
    let id = start_job(&s, "campaign", move |ws, ctl| {
        let c = run_campaign(ws, p, ctl)?;
        let report = c.report_markdown();
        Ok((serde_json::to_value(&c)?, report))
    });
    Ok(Json(serde_json::json!({"id": id, "status": "running"})))
}

async fn campaigns_list(
    State(s): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    let jobs: Vec<JobView> = s
        .jobs
        .lock()
        .values()
        .filter(|j| j.kind == "campaign")
        .map(job_view)
        .map(|mut v| {
            v.result = None;
            v.report_markdown = None;
            v
        })
        .collect();
    let archived = list_campaigns(&s.workspace)?;
    Ok(Json(
        serde_json::json!({"jobs": jobs, "archived": archived}),
    ))
}

async fn job_get(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<JobView> {
    check_key(&s, &headers)?;
    if let Some(j) = s.jobs.lock().get(&id) {
        return Ok(Json(job_view(j)));
    }
    // archived campaign
    let c = load_campaign(&s.workspace, &id)?;
    Ok(Json(JobView {
        id,
        kind: "campaign".into(),
        status: JobStatus::Done,
        started_at: c
            .events
            .first()
            .map(|e| e.timestamp)
            .unwrap_or_else(chrono::Utc::now),
        report_markdown: Some(c.report_markdown()),
        result: Some(serde_json::to_value(&c).map_err(SimorphError::from)?),
        error: None,
    }))
}

async fn job_events(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    let (status, start) = {
        let jobs = s.jobs.lock();
        match jobs.get(&id) {
            Some(j) => (j.status, j.ledger_start),
            None => {
                drop(jobs);
                let c = load_campaign(&s.workspace, &id)?;
                return Ok(Json(
                    serde_json::json!({"status": "done", "events": c.events}),
                ));
            }
        }
    };
    // Live: provenance records appended since the job started.
    let records: Vec<serde_json::Value> = s
        .workspace
        .ledger
        .all()
        .into_iter()
        .skip(start)
        .map(|r| serde_json::json!({"sequence": r.sequence, "timestamp": r.timestamp, "kind": r.kind, "label": r.label}))
        .collect();
    let events = s
        .jobs
        .lock()
        .get(&id)
        .and_then(|j| j.result.as_ref())
        .and_then(|r| r.get("events").cloned());
    Ok(Json(
        serde_json::json!({"status": status, "provenance": records, "events": events}),
    ))
}

async fn job_cancel(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    check_write(&s, &headers)?;
    let jobs = s.jobs.lock();
    let j = jobs
        .get(&id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("job {id}")))?;
    j.control.cancel.cancel();
    Ok(Json(
        serde_json::json!({"id": id, "cancel_requested": true}),
    ))
}

#[derive(Deserialize)]
struct RunReq {
    component: String,
    #[serde(default)]
    inputs: IndexMap<String, serde_json::Value>,
    #[serde(default = "one")]
    fidelity: f64,
    seed: Option<u64>,
}
fn one() -> f64 {
    1.0
}

async fn run_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<RunReq>,
) -> ApiResult<RunOutput> {
    check_write(&s, &headers)?;
    let st = s.clone();
    let out = blocking(move || {
        execute_component(
            Some(&st.workspace),
            &r.component,
            r.inputs,
            r.fidelity,
            r.seed,
            &ExecutionControl::default(),
        )
    })
    .await?;
    s.metrics.inc("simorph_runs_total");
    Ok(Json(out))
}

async fn experiments_list(State(s): State<Shared>, headers: HeaderMap) -> ApiResult<Vec<String>> {
    check_key(&s, &headers)?;
    Ok(Json(
        s.workspace
            .archive
            .list()?
            .into_iter()
            .map(|i| i.to_string())
            .collect(),
    ))
}

async fn experiment_reproduce(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<ReproduceOutput> {
    check_write(&s, &headers)?;
    let st = s.clone();
    Ok(Json(
        blocking(move || reproduce_experiment(&st.workspace, &id, &ExecutionControl::default()))
            .await?,
    ))
}

async fn recommend_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(p): Json<ProblemSpec>,
) -> ApiResult<Recommendation> {
    check_key(&s, &headers)?;
    Ok(Json(blocking(move || recommend_methods(&p)).await?))
}

#[derive(Deserialize)]
struct SobolReq {
    component: String,
    inputs: IndexMap<String, serde_json::Value>,
    vary: IndexMap<String, f64>,
    output: String,
    #[serde(default = "default_n")]
    n: usize,
}
fn default_n() -> usize {
    512
}

async fn sobol_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<SobolReq>,
) -> ApiResult<SobolOutput> {
    check_write(&s, &headers)?;
    Ok(Json(
        blocking(move || {
            sobol(
                &r.component,
                r.inputs,
                r.vary,
                &r.output,
                r.n.min(8192),
                &ExecutionControl::default(),
            )
        })
        .await?,
    ))
}

async fn lab_suites(State(s): State<Shared>, headers: HeaderMap) -> ApiResult<Vec<lab::SuiteInfo>> {
    check_key(&s, &headers)?;
    Ok(Json(lab::suites()?))
}

#[derive(Deserialize)]
struct LabReq {
    suite: String,
    #[serde(default = "two")]
    iterations: usize,
}
fn two() -> usize {
    2
}

async fn lab_run(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<LabReq>,
) -> ApiResult<serde_json::Value> {
    check_write(&s, &headers)?;
    let id = start_job(&s, "lab", move |ws, ctl| {
        let out = lab::run_lab(Some(ws), &r.suite, r.iterations.min(20), ctl)?;
        let report = out.report_markdown.clone();
        Ok((serde_json::to_value(&out)?, report))
    });
    Ok(Json(serde_json::json!({"id": id, "status": "running"})))
}

#[derive(Deserialize)]
struct ProvQuery {
    kind: Option<String>,
    limit: Option<usize>,
}

async fn provenance_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<ProvQuery>,
) -> ApiResult<ProvenanceListing> {
    check_key(&s, &headers)?;
    Ok(Json(provenance_list(
        &s.workspace,
        q.kind.as_deref(),
        q.limit.unwrap_or(100).min(2000),
    )))
}

async fn provenance_explain_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    Ok(Json(
        serde_json::json!({"markdown": provenance_explain(&s.workspace, &id)?}),
    ))
}

async fn provenance_verify_route(
    State(s): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    match s.workspace.ledger.verify_chain() {
        Ok(()) => Ok(Json(
            serde_json::json!({"intact": true, "records": s.workspace.ledger.len()}),
        )),
        Err(e) => Ok(Json(
            serde_json::json!({"intact": false, "error": e.to_string()}),
        )),
    }
}

async fn schemas_route(
    State(s): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    // Generated into a private temporary directory, read back, then removed:
    // a GET has no lasting side effect.
    let dir = std::env::temp_dir().join(format!("simorph-schemas-{}", RunId::new()));
    std::fs::create_dir_all(&dir).map_err(SimorphError::from)?;
    let files = export_schemas(&dir);
    let files = match files {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e.into());
        }
    };
    let mut out = serde_json::Map::new();
    for f in files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&f).map_err(SimorphError::from)?)
                .map_err(SimorphError::from)?;
        out.insert(name, v);
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(Json(serde_json::Value::Object(out)))
}

async fn bench_route(State(s): State<Shared>, headers: HeaderMap) -> ApiResult<serde_json::Value> {
    check_write(&s, &headers)?;
    let id = start_job(&s, "bench", move |_ws, ctl| {
        let r = bench::run(ctl)?;
        let md = bench::to_markdown(&r);
        Ok((serde_json::to_value(&r)?, md))
    });
    Ok(Json(serde_json::json!({"id": id, "status": "running"})))
}

async fn jobs_list(State(s): State<Shared>, headers: HeaderMap) -> ApiResult<Vec<JobView>> {
    check_key(&s, &headers)?;
    Ok(Json(
        s.jobs
            .lock()
            .values()
            .map(job_view)
            .map(|mut v| {
                v.result = None;
                v
            })
            .collect(),
    ))
}

async fn model_assemble_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(p): Json<ProblemSpec>,
) -> ApiResult<AssemblyOutput> {
    check_key(&s, &headers)?;
    Ok(Json(blocking(move || assemble_model(&p)).await?))
}

async fn model_equations_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<EquationsRequest>,
) -> ApiResult<EquationsOutput> {
    check_write(&s, &headers)?;
    Ok(Json(
        blocking(move || model_from_equations(&r, &ExecutionControl::default())).await?,
    ))
}

async fn simulator_generate_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<GenerateRequest>,
) -> ApiResult<serde_json::Value> {
    check_write(&s, &headers)?;
    let id = start_job(&s, "simulator", move |ws, ctl| {
        let out = generate_simulator(ws, &r, ctl)?;
        let md = out.markdown.clone();
        Ok((serde_json::to_value(&out.report)?, md))
    });
    Ok(Json(serde_json::json!({"id": id, "status": "running"})))
}

async fn knowledge_memory_route(
    State(s): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    let st = s.clone();
    let md = blocking(move || memory_report(&st.workspace)).await?;
    Ok(Json(serde_json::json!({"markdown": md})))
}

#[derive(Deserialize)]
struct TransferQuery {
    domain: String,
}

async fn knowledge_transfers_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<TransferQuery>,
) -> ApiResult<Vec<TransferView>> {
    check_key(&s, &headers)?;
    let st = s.clone();
    Ok(Json(
        blocking(move || transfers(&st.workspace, &q.domain)).await?,
    ))
}

/// CORS: only explicitly configured origins; by default no cross-origin
/// access at all (the interface is served from the same origin).  This keeps
/// arbitrary web pages from driving a local server that can run code.
fn cors_layer(origins: &[String]) -> tower_http::cors::CorsLayer {
    use axum::http::{HeaderValue, Method};
    let list: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    tower_http::cors::CorsLayer::new()
        .allow_origin(list)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderName::from_static("x-api-key"),
        ])
}

async fn capabilities_route(
    State(s): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    let engines = blocking(engine_availability).await?;
    Ok(Json(serde_json::json!({"engines": engines})))
}

async fn formal_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<FormalRequest>,
) -> ApiResult<FormalOutput> {
    check_write(&s, &headers)?;
    let st = s.clone();
    Ok(Json(
        blocking(move || formal_check(Some(&st.workspace), &r)).await?,
    ))
}

#[derive(Deserialize)]
struct ImproveReq {
    target: String,
    #[serde(default)]
    recommended: bool,
}

async fn improvement_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<ImproveReq>,
) -> ApiResult<serde_json::Value> {
    check_write(&s, &headers)?;
    let id = start_job(&s, "improvement", move |ws, ctl| {
        let rep = run_improvement(ws, &r.target, r.recommended, ctl)?;
        Ok((serde_json::from_str(&rep.to_json())?, rep.to_markdown()))
    });
    Ok(Json(serde_json::json!({"id": id, "status": "running"})))
}

#[derive(Deserialize)]
struct DiscoverReq {
    /// CSV text with `name [unit]` headers, or empty for the Kepler demonstration.
    #[serde(default)]
    csv: String,
    #[serde(default)]
    target: String,
    #[serde(default = "one_seed")]
    seed: u64,
    #[serde(default = "default_tol")]
    tolerance: f64,
}
fn default_tol() -> f64 {
    0.01
}
fn one_seed() -> u64 {
    11
}

async fn discovery_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<DiscoverReq>,
) -> ApiResult<serde_json::Value> {
    check_write(&s, &headers)?;
    let id = start_job(&s, "discovery", move |ws, ctl| {
        let out = if r.csv.trim().is_empty() {
            discover_kepler(Some(ws), r.seed, ctl)?
        } else {
            discover_csv(Some(ws), &r.csv, &r.target, r.seed, r.tolerance, ctl)?
        };
        let md = format!("**{}** : {}\n\n- loi : `{}`\n- erreur hors échantillon : {:?}\n- homogène en dimension : {:?}\n- niveau : {}\n- {} générations, {} évaluations, {:.1} s", out.title, out.statement, out.law.clone().unwrap_or_else(|| "aucune".into()), out.held_out_error, out.dimension_ok, out.claim_level.label_fr(), out.generations, out.evaluations, out.wall_time_s);
        Ok((serde_json::to_value(&out)?, md))
    });
    Ok(Json(serde_json::json!({"id": id, "status": "running"})))
}

#[derive(Deserialize)]
struct TextReq {
    text: String,
}

async fn understand_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<TextReq>,
) -> ApiResult<serde_json::Value> {
    check_key(&s, &headers)?;
    let u = blocking(move || understand_text(&r.text)).await?;
    Ok(Json(serde_json::json!({
        "spec": u.spec,
        "result": serde_json::to_value(&u).map_err(SimorphError::from)?,
        "markdown": u.to_markdown(),
    })))
}

async fn agents_run_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<TextReq>,
) -> ApiResult<serde_json::Value> {
    check_write(&s, &headers)?;
    let id = start_job(&s, "agents", move |ws, _ctl| {
        let out = run_agent_session(ws, &r.text)?;
        let md = out.report_markdown.clone();
        Ok((serde_json::to_value(&out)?, md))
    });
    Ok(Json(serde_json::json!({"id": id, "status": "running"})))
}

#[derive(Deserialize)]
struct ReplayReq {
    /// Session directory name under `<workspace>/agents/` (no paths accepted).
    session: String,
}

async fn agents_replay_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<ReplayReq>,
) -> ApiResult<serde_json::Value> {
    check_write(&s, &headers)?;
    if r.session.is_empty() || !r.session.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(SimorphError::InvalidInput("session must be a session id".into()).into());
    }
    let st = s.clone();
    let rep = blocking(move || {
        let trace = st
            .workspace
            .root
            .join("agents")
            .join(&r.session)
            .join("trace.jsonl");
        replay_agent_session(&st.workspace, &trace)
    })
    .await?;
    Ok(Json(
        serde_json::to_value(&rep).map_err(SimorphError::from)?,
    ))
}

/// Uploaded file (text content or base64 bytes), stored under
/// `<workspace>/uploads/<sha256>.<ext>`; client file names never reach the
/// file system beyond their extension.
#[derive(Deserialize)]
struct UploadReq {
    filename: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    content_base64: Option<String>,
    #[serde(default = "five")]
    rows: usize,
}
fn five() -> usize {
    5
}

fn store_upload(ws: &Workspace, r: &UploadReq, allowed: &[&str]) -> Result<std::path::PathBuf> {
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    let ext = std::path::Path::new(&r.filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| allowed.contains(&e.as_str()))
        .ok_or_else(|| {
            SimorphError::InvalidInput(format!("file extension must be one of {allowed:?}"))
        })?;
    let bytes = match (&r.content, &r.content_base64) {
        (Some(t), None) => t.as_bytes().to_vec(),
        (None, Some(b)) => base64::engine::general_purpose::STANDARD
            .decode(b.trim())
            .map_err(|e| SimorphError::InvalidInput(format!("invalid base64: {e}")))?,
        _ => {
            return Err(SimorphError::InvalidInput(
                "give exactly one of content or content_base64".into(),
            ))
        }
    };
    let dir = ws.root.join("uploads");
    std::fs::create_dir_all(&dir)?;
    let p = dir.join(format!("{}.{ext}", hex::encode(Sha256::digest(&bytes))));
    std::fs::write(&p, &bytes)?;
    Ok(p)
}

async fn evaluation_status_route(
    State(s): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<EvaluationStatus> {
    check_key(&s, &headers)?;
    Ok(Json(blocking(evaluation_status).await?))
}

async fn data_inspect_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<UploadReq>,
) -> ApiResult<DataInspection> {
    check_write(&s, &headers)?;
    let st = s.clone();
    Ok(Json(
        blocking(move || {
            let p = store_upload(&st.workspace, &r, &["csv", "tsv", "jsonl", "ndjson"])?;
            inspect_data(Some(&st.workspace), &p, r.rows)
        })
        .await?,
    ))
}

async fn fmu_inspect_route(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(r): Json<UploadReq>,
) -> ApiResult<FmuInspection> {
    check_write(&s, &headers)?;
    let st = s.clone();
    Ok(Json(
        blocking(move || {
            let p = store_upload(&st.workspace, &r, &["fmu"])?;
            inspect_fmu(&p)
        })
        .await?,
    ))
}

pub fn build_app(state: Shared) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/api/actions", get(actions))
        .route("/api/components", get(components_list))
        .route("/api/capabilities", get(capabilities_route))
        .route("/api/formal/check", post(formal_route))
        .route("/api/understand", post(understand_route))
        .route("/api/data/inspect", post(data_inspect_route))
        .route("/api/evaluation/status", get(evaluation_status_route))
        .route("/api/fmu/inspect", post(fmu_inspect_route))
        .route("/api/agents/run", post(agents_run_route))
        .route("/api/agents/replay", post(agents_replay_route))
        .route("/api/improvement/run", post(improvement_route))
        .route("/api/discovery", post(discovery_route))
        .route("/api/components/:name", get(component_show))
        .route("/api/units/convert", post(units_convert))
        .route("/api/units/check", post(units_check))
        .route("/api/problems/examples", get(examples_list))
        .route("/api/problems/examples/:name", get(example_get))
        .route("/api/problems/validate", post(problem_validate))
        .route("/api/analyse", post(analyse_route))
        .route("/api/plan", post(plan_route))
        .route("/api/campaigns", post(campaign_start).get(campaigns_list))
        .route("/api/campaigns/:id", get(job_get))
        .route("/api/campaigns/:id/events", get(job_events))
        .route("/api/campaigns/:id/cancel", post(job_cancel))
        .route("/api/jobs", get(jobs_list))
        .route("/api/jobs/:id", get(job_get))
        .route("/api/jobs/:id/events", get(job_events))
        .route("/api/jobs/:id/cancel", post(job_cancel))
        .route("/api/runs", post(run_route))
        .route("/api/experiments", get(experiments_list))
        .route("/api/experiments/:id/reproduce", post(experiment_reproduce))
        .route("/api/optimisation/recommend", post(recommend_route))
        .route("/api/uncertainty/sobol", post(sobol_route))
        .route("/api/lab/suites", get(lab_suites))
        .route("/api/lab/run", post(lab_run))
        .route("/api/provenance", get(provenance_route))
        .route("/api/provenance/verify", get(provenance_verify_route))
        .route("/api/provenance/:id/explain", get(provenance_explain_route))
        .route("/api/schemas", get(schemas_route))
        .route("/api/bench", post(bench_route))
        .route("/api/models/assemble", post(model_assemble_route))
        .route("/api/models/from-equations", post(model_equations_route))
        .route("/api/simulators/generate", post(simulator_generate_route))
        .route("/api/knowledge/memory", get(knowledge_memory_route))
        .route("/api/knowledge/transfers", get(knowledge_transfers_route))
        .layer(cors_layer(&state.cors_origins))
        .with_state(state)
}
