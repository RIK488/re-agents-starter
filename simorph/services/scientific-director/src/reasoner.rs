//! Optional reasoning back-end.
//!
//! A [`Reasoner`] may *propose* hypotheses, disciplines and narrative
//! summaries.  It has no authority: it cannot change evidence levels, success
//! criteria, trade-off weights or protected references, because none of those
//! are reachable from this trait.  The default implementation is rule-based
//! and fully deterministic; an LLM-backed implementation plugs in through the
//! same trait (see `LlmReasonerConfig`).

use crate::analysis::ProblemAnalysis;
use simorph_core::{Hypothesis, ProblemSpec};

pub trait Reasoner: Send + Sync {
    fn name(&self) -> &str;
    /// Additional hypotheses, each with a falsification criterion.
    fn propose_hypotheses(
        &self,
        problem: &ProblemSpec,
        analysis: &ProblemAnalysis,
    ) -> Vec<Hypothesis>;
    /// Narrative summary of the analysis (free text; never a source of truth).
    fn summarise(&self, problem: &ProblemSpec, analysis: &ProblemAnalysis) -> String;
}

/// Deterministic rule-based reasoner.
#[derive(Default)]
pub struct RuleBasedReasoner;

impl Reasoner for RuleBasedReasoner {
    fn name(&self) -> &str {
        "rule-based"
    }
    fn propose_hypotheses(
        &self,
        _problem: &ProblemSpec,
        _analysis: &ProblemAnalysis,
    ) -> Vec<Hypothesis> {
        vec![]
    }
    fn summarise(&self, problem: &ProblemSpec, analysis: &ProblemAnalysis) -> String {
        format!(
            "Problème « {} » : {} grandeurs évaluées, {} solutions candidates, {} disciplines.",
            problem.title,
            analysis.assessments.len(),
            analysis.solution_candidates.len(),
            analysis.disciplines.len()
        )
    }
}

/// Configuration for an LLM-backed reasoner (OpenAI-compatible or Anthropic
/// Messages API).  The HTTP client lives in the application layer
/// (`apps/`), which injects a `Box<dyn Reasoner>`; this crate only defines the
/// contract so that the director never links a network client.
#[derive(Debug, Clone)]
pub struct LlmReasonerConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key_env: String,
    pub max_tokens: u32,
}
