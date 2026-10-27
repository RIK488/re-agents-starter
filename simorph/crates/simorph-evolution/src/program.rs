//! Evolution of real Python programs.
//!
//! * [`Program`]: candidate source text with lineage.
//! * [`Mutator`]: proposes children of a parent given evaluation feedback.
//!   [`SyntacticMutator`] is deterministic (seeded) and works on lines,
//!   operators, constants and a snippet bank; [`LlmMutator`] asks an
//!   OpenAI-compatible or Anthropic Messages endpoint configured by
//!   `SIMORPH_LLM_URL`, `SIMORPH_LLM_MODEL`, `SIMORPH_LLM_API_KEY`
//!   (`SIMORPH_LLM_STYLE` = `openai` | `anthropic` optionally).
//! * [`PythonSandbox`]: runs candidates out of process with layered
//!   restrictions — a private network namespace (`unshare -n`, when the
//!   kernel allows it), the strict `SandboxPolicy` of `simorph-security`
//!   (scrubbed environment, timeout, output cap, dedicated working
//!   directory), and a restricted-execution harness (no imports, no dunder
//!   access, minimal builtins).
//! * [`ProgramTask`]: the task supplies *sealed* instance sets and an
//!   independent Rust scorer; candidates only return raw solutions.
//! * [`evolve_programs`]: elitist archive + tournament parent selection,
//!   held-out evaluation of the seed and of the final best only.

use parking_lot::Mutex;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_execution::SubprocessComponent;
use simorph_provenance::{Ledger, ProvenanceKind};
use simorph_security::SandboxPolicy;
use std::path::PathBuf;

const HARNESS: &str = include_str!("harness.py");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub id: String,
    pub source: String,
    pub parent: Option<String>,
    /// Which mutator / operation produced it.
    pub origin: String,
}

impl Program {
    pub fn new(
        source: impl Into<String>,
        parent: Option<&Program>,
        origin: impl Into<String>,
    ) -> Self {
        let source = source.into();
        let id = simorph_core::hashing::sha256_hex(source.as_bytes())[..16].to_string();
        Self {
            id,
            source,
            parent: parent.map(|p| p.id.clone()),
            origin: origin.into(),
        }
    }
}

/// Feedback given to mutators (score of the parent and its failures).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalFeedback {
    pub score: f64,
    pub failures: usize,
    pub messages: Vec<String>,
}

pub trait Mutator: Send + Sync {
    fn name(&self) -> &str;
    fn propose(&self, parent: &Program, context: &EvalFeedback) -> Result<Vec<Program>>;
}

// ------------------------------------------------------------------ syntactic mutator

/// Deterministic line/operator/constant/snippet mutations.
pub struct SyntacticMutator {
    rng: Mutex<ChaCha8Rng>,
    pub snippets: Vec<String>,
    pub children_per_call: usize,
}

impl SyntacticMutator {
    pub fn new(seed: u64, snippets: Vec<String>) -> Self {
        Self {
            rng: Mutex::new(ChaCha8Rng::seed_from_u64(seed)),
            snippets,
            children_per_call: 4,
        }
    }

    fn indent_of(line: &str) -> &str {
        &line[..line.len() - line.trim_start().len()]
    }

    fn mutate_once(&self, src: &str, rng: &mut ChaCha8Rng) -> (String, String) {
        let lines: Vec<&str> = src.lines().collect();
        let n = lines.len().max(1);
        match rng.gen_range(0..5) {
            0 if !self.snippets.is_empty() => {
                // insert a snippet before line i with that line's indentation (never before `def`)
                let i = rng.gen_range(1..n.max(2)).min(lines.len());
                let indent = lines.get(i).map(|l| Self::indent_of(l)).unwrap_or("    ");
                let indent = if indent.is_empty() { "    " } else { indent };
                let snip = &self.snippets[rng.gen_range(0..self.snippets.len())];
                let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
                let block: Vec<String> = snip.lines().map(|l| format!("{indent}{l}")).collect();
                for (k, b) in block.into_iter().enumerate() {
                    out.insert(i + k, b);
                }
                (
                    out.join("\n") + "\n",
                    format!(
                        "insert snippet `{}` at line {}",
                        snip.lines().next().unwrap_or(""),
                        i + 1
                    ),
                )
            }
            1 if n > 2 => {
                let i = rng.gen_range(1..n);
                if lines[i].trim_start().starts_with("def ")
                    || lines[i].trim_start().starts_with("return")
                {
                    return (src.to_string(), "no-op".into());
                }
                let out: Vec<&str> = lines
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| *k != i)
                    .map(|(_, l)| *l)
                    .collect();
                (out.join("\n") + "\n", format!("delete line {}", i + 1))
            }
            2 => {
                let swaps = [
                    (">=", ">"),
                    (">", ">="),
                    ("<=", "<"),
                    ("<", "<="),
                    ("+", "-"),
                    ("-", "+"),
                    ("min(", "max("),
                    ("max(", "min("),
                ];
                let (a, b) = swaps[rng.gen_range(0..swaps.len())];
                let positions: Vec<usize> = src.match_indices(a).map(|(p, _)| p).collect();
                if positions.is_empty() {
                    return (src.to_string(), "no-op".into());
                }
                let p = positions[rng.gen_range(0..positions.len())];
                (
                    format!("{}{}{}", &src[..p], b, &src[p + a.len()..]),
                    format!("swap `{a}`→`{b}`"),
                )
            }
            3 => {
                let re = regex::Regex::new(r"\b\d+(\.\d+)?\b").expect("regex");
                let ms: Vec<regex::Match> = re.find_iter(src).collect();
                if ms.is_empty() {
                    return (src.to_string(), "no-op".into());
                }
                let m = ms[rng.gen_range(0..ms.len())];
                let v: f64 = m.as_str().parse().unwrap_or(1.0);
                let nv = if m.as_str().contains('.') {
                    format!("{:.3}", v * rng.gen_range(0.5..1.5))
                } else {
                    format!("{}", (v as i64 + rng.gen_range(-1..=1)).max(0))
                };
                (
                    format!("{}{}{}", &src[..m.start()], nv, &src[m.end()..]),
                    format!("constant {} → {nv}", m.as_str()),
                )
            }
            _ if n > 3 => {
                let i = rng.gen_range(1..n - 1);
                let mut out: Vec<&str> = lines.clone();
                out.swap(i, i + 1);
                (
                    out.join("\n") + "\n",
                    format!("swap lines {} and {}", i + 1, i + 2),
                )
            }
            _ => (src.to_string(), "no-op".into()),
        }
    }
}

impl Mutator for SyntacticMutator {
    fn name(&self) -> &str {
        "syntactic"
    }
    fn propose(&self, parent: &Program, _context: &EvalFeedback) -> Result<Vec<Program>> {
        let mut rng = self.rng.lock();
        let mut out = vec![];
        for _ in 0..self.children_per_call {
            let (s, why) = self.mutate_once(&parent.source, &mut rng);
            if s != parent.source {
                out.push(Program::new(s, Some(parent), format!("syntactic: {why}")));
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------ LLM mutator

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmStyle {
    OpenAi,
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Full endpoint URL (e.g. `https://api.anthropic.com/v1/messages` or
    /// `http://localhost:8000/v1/chat/completions`).
    pub url: String,
    pub model: String,
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
    pub style: LlmStyle,
    pub max_tokens: u32,
    pub timeout_s: u64,
}

impl LlmConfig {
    /// Read the configuration from the environment; `NotImplemented` (with
    /// the reason) when no endpoint is configured.
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("SIMORPH_LLM_URL").ok().filter(|s| !s.is_empty()).ok_or_else(|| {
            SimorphError::NotImplemented("LLM mutator disabled: SIMORPH_LLM_URL is not set (also SIMORPH_LLM_MODEL, SIMORPH_LLM_API_KEY)".into())
        })?;
        let model = std::env::var("SIMORPH_LLM_MODEL").map_err(|_| {
            SimorphError::InvalidInput("SIMORPH_LLM_MODEL must be set with SIMORPH_LLM_URL".into())
        })?;
        let style = match std::env::var("SIMORPH_LLM_STYLE").ok().as_deref() {
            Some("anthropic") => LlmStyle::Anthropic,
            Some("openai") => LlmStyle::OpenAi,
            _ if url.contains("anthropic") || url.ends_with("/v1/messages") => LlmStyle::Anthropic,
            _ => LlmStyle::OpenAi,
        };
        Ok(Self {
            url,
            model,
            api_key: std::env::var("SIMORPH_LLM_API_KEY").ok(),
            style,
            max_tokens: 2048,
            timeout_s: 120,
        })
    }
}

/// Extract the first fenced Python block (or the whole text if it already
/// looks like code defining `entry`).
pub fn extract_code(text: &str, entry: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?s)```(?:python|py)?\s*\n(.*?)```").expect("regex");
    let code = re.captures(text).map(|c| c[1].to_string()).or_else(|| {
        if text.contains("def ") {
            Some(text.to_string())
        } else {
            None
        }
    })?;
    if code.contains(&format!("def {entry}(")) {
        Some(code)
    } else {
        None
    }
}

pub struct LlmMutator {
    pub config: LlmConfig,
    pub entry: String,
    pub task_description: String,
    pub children_per_call: usize,
}

impl LlmMutator {
    pub fn new(config: LlmConfig, entry: &str, task_description: &str) -> Self {
        Self {
            config,
            entry: entry.into(),
            task_description: task_description.into(),
            children_per_call: 1,
        }
    }
    pub fn from_env(entry: &str, task_description: &str) -> Result<Self> {
        Ok(Self::new(LlmConfig::from_env()?, entry, task_description))
    }

    fn prompt(&self, parent: &Program, fb: &EvalFeedback) -> String {
        format!(
            "You improve a Python function for the following task.\n{}\n\nCurrent program (score {:.4}, lower is better; {} failing instances):\n```python\n{}```\n{}\nConstraints: no imports, no file/network access, no names starting with '_', only builtins and the `math` module. Return the complete improved program defining `{}` in one ```python block.",
            self.task_description,
            fb.score,
            fb.failures,
            parent.source,
            if fb.messages.is_empty() { String::new() } else { format!("Errors observed: {}\n", fb.messages.join("; ")) },
            self.entry
        )
    }

    fn call(&self, prompt: &str) -> Result<String> {
        let local =
            self.config.url.contains("://127.0.0.1") || self.config.url.contains("://localhost");
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(self.config.timeout_s))
            .try_proxy_from_env(!local)
            .build();
        let (body, req) = match self.config.style {
            LlmStyle::OpenAi => {
                let body = serde_json::json!({"model": self.config.model, "max_tokens": self.config.max_tokens, "messages": [{"role": "user", "content": prompt}]});
                let mut r = agent
                    .post(&self.config.url)
                    .set("content-type", "application/json");
                if let Some(k) = &self.config.api_key {
                    r = r.set("authorization", &format!("Bearer {k}"));
                }
                (body, r)
            }
            LlmStyle::Anthropic => {
                let body = serde_json::json!({"model": self.config.model, "max_tokens": self.config.max_tokens, "messages": [{"role": "user", "content": prompt}]});
                let mut r = agent
                    .post(&self.config.url)
                    .set("content-type", "application/json")
                    .set("anthropic-version", "2023-06-01");
                if let Some(k) = &self.config.api_key {
                    r = r.set("x-api-key", k);
                }
                (body, r)
            }
        };
        let resp = req.send_json(body).map_err(|e| SimorphError::Component {
            component: "llm-mutator".into(),
            message: format!("HTTP call failed: {e}"),
        })?;
        let v: serde_json::Value = resp
            .into_json()
            .map_err(|e| SimorphError::Serialization(format!("LLM response is not JSON: {e}")))?;
        let text = match self.config.style {
            LlmStyle::OpenAi => v["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.to_string()),
            LlmStyle::Anthropic => v["content"].as_array().map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            }),
        };
        text.ok_or_else(|| {
            SimorphError::Serialization(format!("unexpected LLM response shape: {v}"))
        })
    }
}

impl Mutator for LlmMutator {
    fn name(&self) -> &str {
        "llm"
    }
    fn propose(&self, parent: &Program, context: &EvalFeedback) -> Result<Vec<Program>> {
        let prompt = self.prompt(parent, context);
        let mut out = vec![];
        for _ in 0..self.children_per_call {
            let text = self.call(&prompt)?;
            if let Some(code) = extract_code(&text, &self.entry) {
                out.push(Program::new(
                    code,
                    Some(parent),
                    format!("llm: {}", self.config.model),
                ));
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------ sandbox

/// Command prefix giving the child a private network namespace, if the
/// kernel permits it here; returns `(prefix, isolated)`.
pub fn network_isolation_prefix() -> (Vec<String>, bool) {
    let ok = std::process::Command::new("/usr/bin/unshare")
        .args(["-n", "true"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok {
        (
            vec!["/usr/bin/unshare".into(), "-n".into(), "--".into()],
            true,
        )
    } else {
        (vec![], false)
    }
}

/// Raw output of one sandboxed run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxOutput {
    pub solutions: Vec<serde_json::Value>,
    pub errors: Vec<Option<String>>,
    pub fatal: Option<String>,
}

pub struct PythonSandbox {
    component: SubprocessComponent,
    pub dir: PathBuf,
    pub network_isolated: bool,
    policy: SandboxPolicy,
    entry: String,
}

impl PythonSandbox {
    /// Start a warm sandbox for programs defining `entry`.
    pub fn new(entry: &str, timeout_s: f64) -> Result<Self> {
        let dir = std::env::temp_dir().join(format!("simorph-evo-{}", RunId::new()));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("harness.py"), HARNESS)?;
        let manifest = ComponentManifest::in_process(
            "evolution.python_program_sandbox",
            "0.1.0",
            "simorph-evolution",
        )
        .with_capabilities(vec![Capability::Evaluation])
        .with_inputs(vec![
            Port::scalar("source", "1"),
            Port::scalar("instances", "1"),
        ])
        .with_outputs(vec![
            Port::scalar("solutions", "1"),
            Port::scalar("errors", "1"),
            Port::scalar("fatal", "1"),
        ]);
        std::fs::write(dir.join("manifest.json"), serde_json::to_vec(&manifest)?)?;
        let policy = SandboxPolicy::strict()
            .with_timeout(timeout_s)
            .with_working_dir(&dir)
            .with_max_output(4 * 1024 * 1024);
        let (prefix, isolated) = network_isolation_prefix();
        let py = simorph_adapter_python_bridge::python_interpreter();
        let mut argv: Vec<String> = prefix;
        argv.push(py);
        argv.extend([
            "-I".to_string(),
            dir.join("harness.py").to_string_lossy().into_owned(),
            dir.join("manifest.json").to_string_lossy().into_owned(),
            entry.to_string(),
        ]);
        let cmd = argv.remove(0);
        let component =
            SubprocessComponent::new(cmd, argv, policy.clone())?.with_max_restarts(1_000);
        Ok(Self {
            component,
            dir,
            network_isolated: isolated,
            policy,
            entry: entry.into(),
        })
    }

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }
    pub fn entry(&self) -> &str {
        &self.entry
    }

    /// Run `source` on `instances` (a JSON array of keyword-argument objects).
    /// A timeout or crash is reported as a fatal error, not as a Rust error.
    pub fn run(
        &self,
        source: &str,
        instances: &serde_json::Value,
        ctl: &ExecutionControl,
    ) -> SandboxOutput {
        let n = instances.as_array().map(|a| a.len()).unwrap_or(0);
        let req = TaskRequest::new(self.component.manifest().id)
            .input("source", source)
            .input("instances", instances.clone());
        match self.component.execute(&req, ctl) {
            Ok(r) => SandboxOutput {
                solutions: r
                    .outputs
                    .get("solutions")
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_else(|| vec![serde_json::Value::Null; n]),
                errors: r
                    .outputs
                    .get("errors")
                    .and_then(|v| v.as_array().cloned())
                    .map(|a| {
                        a.into_iter()
                            .map(|e| e.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_else(|| vec![None; n]),
                fatal: r
                    .outputs
                    .get("fatal")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            },
            Err(e) => SandboxOutput {
                solutions: vec![serde_json::Value::Null; n],
                errors: vec![Some(e.to_string()); n],
                fatal: Some(e.to_string()),
            },
        }
    }
}

impl Drop for PythonSandbox {
    fn drop(&mut self) {
        self.component.shutdown();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ------------------------------------------------------------------ tasks and evolution

/// An instance set sealed by its hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedInstances {
    pub name: String,
    pub instances: serde_json::Value,
    pub hash: String,
}

impl SealedInstances {
    pub fn seal(name: &str, instances: serde_json::Value) -> Self {
        let hash = simorph_core::hashing::hash_json(&instances).expect("json");
        Self {
            name: name.into(),
            instances,
            hash,
        }
    }
    pub fn verify(&self) -> Result<()> {
        if simorph_core::hashing::hash_json(&self.instances)? != self.hash {
            return Err(SimorphError::Protected(format!(
                "instance set `{}` modified after sealing",
                self.name
            )));
        }
        Ok(())
    }
}

/// Score of one program on one instance set (lower is better).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskScore {
    pub score: f64,
    pub failures: usize,
    pub per_instance: Vec<f64>,
    pub messages: Vec<String>,
}

/// A program-evolution task: entry point, sealed instance sets and an
/// independent scorer that recomputes quality from raw solutions.
pub trait ProgramTask: Send + Sync {
    fn name(&self) -> &str;
    fn entry_point(&self) -> &str;
    fn description(&self) -> &str;
    fn train(&self) -> &SealedInstances;
    fn held_out(&self) -> &SealedInstances;
    fn score(&self, set: &SealedInstances, out: &SandboxOutput) -> TaskScore;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredProgram {
    pub program: Program,
    pub train: TaskScore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramEvolutionConfig {
    pub generations: usize,
    pub archive_size: usize,
    pub parents_per_generation: usize,
    pub tournament: usize,
    pub seed: u64,
}

impl Default for ProgramEvolutionConfig {
    fn default() -> Self {
        Self {
            generations: 12,
            archive_size: 8,
            parents_per_generation: 3,
            tournament: 2,
            seed: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramEvolutionResult {
    pub seed_train: TaskScore,
    pub seed_held_out: TaskScore,
    pub best: ScoredProgram,
    pub best_held_out: TaskScore,
    pub history: Vec<(usize, f64)>,
    pub evaluated: usize,
    pub failed_candidates: usize,
    pub train_hash: String,
    pub held_out_hash: String,
    pub network_isolated: bool,
    pub wall_time_s: f64,
}

fn evaluate(
    task: &dyn ProgramTask,
    sandbox: &PythonSandbox,
    set: &SealedInstances,
    p: &Program,
    ctl: &ExecutionControl,
) -> Result<TaskScore> {
    set.verify()?;
    let out = sandbox.run(&p.source, &set.instances, ctl);
    let s = task.score(set, &out);
    // the sealed set must be intact after the run as well
    set.verify()?;
    Ok(s)
}

pub fn evolve_programs(
    task: &dyn ProgramTask,
    seed_program: Program,
    mutators: &[&dyn Mutator],
    sandbox: &PythonSandbox,
    cfg: &ProgramEvolutionConfig,
    ledger: &Ledger,
    ctl: &ExecutionControl,
) -> Result<ProgramEvolutionResult> {
    let start = std::time::Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(cfg.seed);
    let root = ledger.record(ProvenanceKind::Experiment, format!("évolution de programmes — {}", task.name()), &serde_json::json!({"train_hash": task.train().hash, "held_out_hash": task.held_out().hash, "network_isolated": sandbox.network_isolated, "config": cfg}), &[])?;
    let seed_train = evaluate(task, sandbox, task.train(), &seed_program, ctl)?;
    let seed_held_out = evaluate(task, sandbox, task.held_out(), &seed_program, ctl)?;
    let mut archive: Vec<ScoredProgram> = vec![ScoredProgram {
        program: seed_program.clone(),
        train: seed_train.clone(),
    }];
    let mut seen: std::collections::HashSet<String> =
        [seed_program.id.clone()].into_iter().collect();
    let mut history = vec![(0, seed_train.score)];
    let mut evaluated = 1;
    let mut failed = 0;
    for g in 1..=cfg.generations {
        ctl.check()?;
        let mut children = vec![];
        for _ in 0..cfg.parents_per_generation {
            let mut best = &archive[rng.gen_range(0..archive.len())];
            for _ in 1..cfg.tournament {
                let c = &archive[rng.gen_range(0..archive.len())];
                if c.train.score < best.train.score {
                    best = c;
                }
            }
            let fb = EvalFeedback {
                score: best.train.score,
                failures: best.train.failures,
                messages: best.train.messages.iter().take(3).cloned().collect(),
            };
            for m in mutators {
                match m.propose(&best.program, &fb) {
                    Ok(ps) => children.extend(ps),
                    Err(e) => {
                        ledger.record(
                            ProvenanceKind::Failure,
                            format!("mutateur {} indisponible", m.name()),
                            &e.to_string(),
                            &[root],
                        )?;
                    }
                }
            }
        }
        for c in children {
            if !seen.insert(c.id.clone()) {
                continue;
            }
            let s = evaluate(task, sandbox, task.train(), &c, ctl)?;
            evaluated += 1;
            if s.failures > 0 {
                failed += 1;
            }
            archive.push(ScoredProgram {
                program: c,
                train: s,
            });
        }
        archive.sort_by(|a, b| {
            a.train
                .score
                .total_cmp(&b.train.score)
                .then(a.program.source.len().cmp(&b.program.source.len()))
        });
        archive.truncate(cfg.archive_size);
        history.push((g, archive[0].train.score));
        ledger.record(ProvenanceKind::Run, format!("génération {g} — meilleur score {:.4}", archive[0].train.score), &serde_json::json!({"generation": g, "best": archive[0].program.id, "score": archive[0].train.score, "origin": archive[0].program.origin}), &[root])?;
    }
    let best = archive[0].clone();
    let best_held_out = evaluate(task, sandbox, task.held_out(), &best.program, ctl)?;
    ledger.record(ProvenanceKind::Model, format!("meilleur programme {}", best.program.id), &serde_json::json!({"source": best.program.source, "train": best.train.score, "held_out": best_held_out.score, "seed_held_out": seed_held_out.score}), &[root])?;
    Ok(ProgramEvolutionResult {
        seed_train,
        seed_held_out,
        best,
        best_held_out,
        history,
        evaluated,
        failed_candidates: failed,
        train_hash: task.train().hash.clone(),
        held_out_hash: task.held_out().hash.clone(),
        network_isolated: sandbox.network_isolated,
        wall_time_s: start.elapsed().as_secs_f64(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_code_requires_entry_point() {
        let t = "Here:\n```python\ndef pack(items, capacity):\n    return []\n```\n";
        assert!(extract_code(t, "pack").unwrap().contains("def pack("));
        assert!(extract_code(t, "solve").is_none());
    }

    #[test]
    fn sealed_instances_detect_modification() {
        let mut s = SealedInstances::seal("x", serde_json::json!([{"a": 1}]));
        s.verify().unwrap();
        s.instances = serde_json::json!([{"a": 2}]);
        assert!(matches!(s.verify(), Err(SimorphError::Protected(_))));
    }

    #[test]
    fn llm_mutator_disabled_without_configuration() {
        if std::env::var("SIMORPH_LLM_URL")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
        {
            return; // configured in this environment: nothing to check
        }
        assert!(matches!(
            LlmConfig::from_env(),
            Err(SimorphError::NotImplemented(_))
        ));
    }

    #[test]
    fn syntactic_mutator_is_deterministic() {
        let p = Program::new("def f(x):\n    y = x + 1\n    return y\n", None, "seed");
        let a = SyntacticMutator::new(3, vec!["y = y * 2".into()])
            .propose(&p, &EvalFeedback::default())
            .unwrap();
        let b = SyntacticMutator::new(3, vec!["y = y * 2".into()])
            .propose(&p, &EvalFeedback::default())
            .unwrap();
        assert_eq!(a, b);
        assert!(a.iter().all(|c| c.parent.as_deref() == Some(p.id.as_str())));
    }
}
