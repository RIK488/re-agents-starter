//! External evaluation harness launcher.
//!
//! Inspect AI, LM Evaluation Harness, LightEval and HELM are not bundled
//! with SIMORPH.  They are driven as subprocesses under a `SandboxPolicy`:
//! the launcher runs a command that must write a JSON results document to
//! stdout (or to a file), and [`ResultParser`] maps that document to
//! `TaskResult`s whose outputs are the harness metrics.  Availability is
//! detected, never assumed.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_security::SandboxPolicy;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HarnessKind {
    InspectAi,
    LmEvalHarness,
    LightEval,
    Helm,
    /// Any command emitting the generic `{"results": {task: {metric: value}}}` document.
    Generic,
}

impl HarnessKind {
    /// Python module whose import proves the harness is installed.
    pub fn probe_module(self) -> Option<&'static str> {
        match self {
            HarnessKind::InspectAi => Some("inspect_ai"),
            HarnessKind::LmEvalHarness => Some("lm_eval"),
            HarnessKind::LightEval => Some("lighteval"),
            HarnessKind::Helm => Some("helm"),
            HarnessKind::Generic => None,
        }
    }
}

/// How to read the harness output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResultParser {
    /// `{"results": {"<task>": {"<metric>": <number>, ...}}}` on stdout
    /// (the shape used by lm-evaluation-harness and, after `--log-format json`, by lighteval).
    ResultsByTask,
    /// Same document read from a file written by the harness.
    ResultsByTaskFile { path: PathBuf },
    /// Inspect AI `eval` log: `{"results": {"scores": [{"name":..., "metrics": {m: {"value": v}}}]}}`.
    InspectLog { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExternalHarness {
    pub kind: HarnessKind,
    /// Full command line (program + args).
    pub command: Vec<String>,
    pub result_parser: ResultParser,
    pub sandbox: SandboxPolicy,
}

/// Probe whether the harness' Python module imports.
pub fn harness_available(kind: HarnessKind) -> std::result::Result<(), String> {
    let Some(module) = kind.probe_module() else {
        return Ok(());
    };
    let out = Command::new(simorph_adapter_python_bridge::python_interpreter())
        .args(["-I", "-c", &format!("import {module}")])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("cannot run python: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{kind:?} unavailable: module `{module}` does not import ({})",
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .last()
                .unwrap_or("")
                .trim()
        ))
    }
}

/// Parse the generic results document into one `TaskResult` per task.
pub fn parse_results_by_task(
    doc: &serde_json::Value,
    component: ComponentId,
) -> Result<Vec<(String, TaskResult)>> {
    let results = doc
        .get("results")
        .and_then(|r| r.as_object())
        .ok_or_else(|| {
            SimorphError::InvalidInput("harness output lacks a `results` object".into())
        })?;
    let mut out = vec![];
    for (task, metrics) in results {
        let obj = metrics.as_object().ok_or_else(|| {
            SimorphError::InvalidInput(format!("task `{task}` metrics is not an object"))
        })?;
        let req = TaskRequest::new(component).input("task", task.clone());
        let mut res = TaskResult::new(&req, EvidenceLevel::Simulated);
        for (k, v) in obj {
            if let Some(x) = v.as_f64() {
                res.outputs.insert(k.clone(), serde_json::Value::from(x));
            }
        }
        if res.outputs.is_empty() {
            return Err(SimorphError::InvalidInput(format!(
                "task `{task}` has no numeric metric"
            )));
        }
        out.push((task.clone(), res));
    }
    Ok(out)
}

/// Parse an Inspect AI eval log.
pub fn parse_inspect_log(
    doc: &serde_json::Value,
    component: ComponentId,
) -> Result<Vec<(String, TaskResult)>> {
    let scores = doc
        .pointer("/results/scores")
        .and_then(|s| s.as_array())
        .ok_or_else(|| SimorphError::InvalidInput("inspect log lacks results.scores".into()))?;
    let task = doc
        .pointer("/eval/task")
        .and_then(|t| t.as_str())
        .unwrap_or("inspect")
        .to_string();
    let mut out = vec![];
    for sc in scores {
        let name = sc.get("name").and_then(|n| n.as_str()).unwrap_or("score");
        let req = TaskRequest::new(component)
            .input("task", task.clone())
            .input("scorer", name);
        let mut res = TaskResult::new(&req, EvidenceLevel::Simulated);
        if let Some(metrics) = sc.get("metrics").and_then(|m| m.as_object()) {
            for (k, v) in metrics {
                if let Some(x) = v
                    .get("value")
                    .and_then(|x| x.as_f64())
                    .or_else(|| v.as_f64())
                {
                    res.outputs.insert(k.clone(), serde_json::Value::from(x));
                }
            }
        }
        out.push((format!("{task}/{name}"), res));
    }
    Ok(out)
}

impl ExternalHarness {
    /// Run the harness under the sandbox policy and parse its results.
    /// Returns `NotImplemented` with the precise reason when the harness is
    /// not installed.
    pub fn run(&self, ctl: &ExecutionControl) -> Result<Vec<(String, TaskResult)>> {
        if let Err(reason) = harness_available(self.kind) {
            return Err(SimorphError::NotImplemented(format!(
                "external harness not runnable: {reason}"
            )));
        }
        self.sandbox.validate()?;
        ctl.check()?;
        let (prog, args) = self
            .command
            .split_first()
            .ok_or_else(|| SimorphError::InvalidInput("empty harness command".into()))?;
        let mut cmd = Command::new(prog);
        cmd.args(args)
            .env_clear()
            .envs(self.sandbox.to_subprocess_env())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(wd) = &self.sandbox.working_dir {
            cmd.current_dir(wd);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| SimorphError::Io(format!("spawn `{prog}`: {e}")))?;
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_secs_f64(if self.sandbox.max_cpu_s > 0.0 {
                self.sandbox.max_cpu_s
            } else {
                3600.0
            });
        loop {
            if let Some(_status) = child.try_wait()? {
                break;
            }
            if ctl.check().is_err() || std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(if ctl.cancel.is_cancelled() {
                    SimorphError::Cancelled
                } else {
                    SimorphError::Timeout((self.sandbox.max_cpu_s * 1000.0) as u64)
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let out = child.wait_with_output()?;
        if out.stdout.len() as u64 > self.sandbox.max_output_bytes {
            return Err(SimorphError::Other(
                "harness output exceeds the sandbox output cap".into(),
            ));
        }
        if !out.status.success() {
            return Err(SimorphError::Component {
                component: format!("{:?}", self.kind),
                message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        let component = ComponentId::deterministic(&format!("harness.{:?}", self.kind));
        let doc: serde_json::Value = match &self.result_parser {
            ResultParser::ResultsByTask => serde_json::from_slice(&out.stdout)?,
            ResultParser::ResultsByTaskFile { path } | ResultParser::InspectLog { path } => {
                serde_json::from_slice(&std::fs::read(path)?)?
            }
        };
        match &self.result_parser {
            ResultParser::InspectLog { .. } => parse_inspect_log(&doc, component),
            _ => parse_results_by_task(&doc, component),
        }
    }

    /// Flatten harness results into `metric name -> value` (task-prefixed).
    pub fn flatten(results: &[(String, TaskResult)]) -> IndexMap<String, f64> {
        let mut m = IndexMap::new();
        for (task, r) in results {
            for (k, v) in &r.outputs {
                if let Some(x) = v.as_f64() {
                    m.insert(format!("{task}/{k}"), x);
                }
            }
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_generic_harness_is_parsed_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake_harness.py");
        std::fs::write(&script, "import json\nprint(json.dumps({'results': {'gsm8k': {'exact_match': 0.42, 'n': 100}, 'arc': {'acc': 0.77}}}))\n").unwrap();
        let h = ExternalHarness {
            kind: HarnessKind::Generic,
            command: vec![
                simorph_adapter_python_bridge::python_interpreter(),
                "-I".into(),
                script.to_string_lossy().to_string(),
            ],
            result_parser: ResultParser::ResultsByTask,
            sandbox: SandboxPolicy::strict().with_timeout(30.0),
        };
        let res = h.run(&ExecutionControl::default()).unwrap();
        let flat = ExternalHarness::flatten(&res);
        assert_eq!(flat["gsm8k/exact_match"], 0.42);
        assert_eq!(flat["arc/acc"], 0.77);
        assert!(res
            .iter()
            .all(|(_, r)| r.evidence == EvidenceLevel::Simulated));
    }

    #[test]
    fn inspect_log_parser_and_missing_harness_detection() {
        let doc = serde_json::json!({"eval": {"task": "mmlu"}, "results": {"scores": [{"name": "choice", "metrics": {"accuracy": {"value": 0.61}, "stderr": {"value": 0.02}}}]}});
        let r = parse_inspect_log(&doc, ComponentId::deterministic("x")).unwrap();
        assert_eq!(r[0].0, "mmlu/choice");
        assert_eq!(r[0].1.get_f64("accuracy").unwrap(), 0.61);
        // real harnesses are not installed in this environment: the launcher must say so
        for kind in [
            HarnessKind::InspectAi,
            HarnessKind::LmEvalHarness,
            HarnessKind::LightEval,
            HarnessKind::Helm,
        ] {
            let h = ExternalHarness {
                kind,
                command: vec!["true".into()],
                result_parser: ResultParser::ResultsByTask,
                sandbox: SandboxPolicy::strict(),
            };
            match h.run(&ExecutionControl::default()) {
                Err(SimorphError::NotImplemented(msg)) => {
                    assert!(msg.contains("does not import"), "{msg}")
                }
                Ok(_) => eprintln!("{kind:?} is installed here; launcher ran"),
                Err(e) => panic!("unexpected error {e}"),
            }
        }
    }
}
