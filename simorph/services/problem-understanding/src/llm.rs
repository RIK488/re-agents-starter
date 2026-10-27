//! Optional LLM proposer.
//!
//! Configured by environment variables:
//!
//! | variable | meaning |
//! |---|---|
//! | `SIMORPH_LLM_URL` | full endpoint URL (e.g. `https://api.anthropic.com/v1/messages` or an OpenAI-compatible `/v1/chat/completions`) |
//! | `SIMORPH_LLM_MODEL` | model identifier |
//! | `SIMORPH_LLM_API_KEY` | API key (never logged) |
//! | `SIMORPH_LLM_API_STYLE` | `anthropic` (default) or `openai` |
//!
//! When unconfigured every call returns `SimorphError::NotImplemented`.
//! An LLM *proposes*; [`validate_against_catalog`] decides what is kept.

use crate::catalog::Catalog;
use crate::understand::{
    Ambiguity, AmbiguityKind, ConceptMapping, MentionRole, UnderstandingResult,
};
use serde::{Deserialize, Serialize};
use simorph_core::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiStyle {
    Anthropic,
    OpenAi,
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub url: String,
    pub model: String,
    pub api_key: String,
    pub style: ApiStyle,
    pub max_tokens: u32,
    pub timeout_s: u64,
}

impl LlmConfig {
    /// Read the configuration; `None` when `SIMORPH_LLM_URL` or `SIMORPH_LLM_MODEL` is missing.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("SIMORPH_LLM_URL")
            .ok()
            .filter(|s| !s.is_empty())?;
        let model = std::env::var("SIMORPH_LLM_MODEL")
            .ok()
            .filter(|s| !s.is_empty())?;
        let api_key = std::env::var("SIMORPH_LLM_API_KEY").unwrap_or_default();
        let style = match std::env::var("SIMORPH_LLM_API_STYLE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "openai" => ApiStyle::OpenAi,
            _ => ApiStyle::Anthropic,
        };
        Some(Self {
            url,
            model,
            api_key,
            style,
            max_tokens: 2048,
            timeout_s: 60,
        })
    }
}

/// Minimal chat-completion client (blocking).
pub struct LlmClient {
    pub config: LlmConfig,
    agent: ureq::Agent,
}

impl std::fmt::Debug for LlmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LlmClient({}, {})", self.config.url, self.config.model)
    }
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> Self {
        let local = config.url.contains("://127.0.0.1") || config.url.contains("://localhost");
        let mut b =
            ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(config.timeout_s));
        if !local {
            b = b.try_proxy_from_env(true);
        }
        Self {
            config,
            agent: b.build(),
        }
    }

    pub fn from_env() -> Result<Self> {
        LlmConfig::from_env().map(Self::new).ok_or_else(|| {
            SimorphError::NotImplemented(
                "LLM not configured (set SIMORPH_LLM_URL and SIMORPH_LLM_MODEL)".into(),
            )
        })
    }

    /// Send one system + user message, return the text answer.
    pub fn complete(&self, system: &str, user: &str) -> Result<String> {
        let c = &self.config;
        let (body, req) = match c.style {
            ApiStyle::Anthropic => (
                serde_json::json!({"model": c.model, "max_tokens": c.max_tokens, "system": system, "messages": [{"role": "user", "content": user}]}),
                self.agent
                    .post(&c.url)
                    .set("x-api-key", &c.api_key)
                    .set("anthropic-version", "2023-06-01")
                    .set("content-type", "application/json"),
            ),
            ApiStyle::OpenAi => (
                serde_json::json!({"model": c.model, "max_tokens": c.max_tokens, "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]}),
                self.agent
                    .post(&c.url)
                    .set("authorization", &format!("Bearer {}", c.api_key))
                    .set("content-type", "application/json"),
            ),
        };
        let resp = req.send_json(body).map_err(|e| SimorphError::Component {
            component: "llm".into(),
            message: e.to_string(),
        })?;
        let v: serde_json::Value = resp
            .into_json()
            .map_err(|e| SimorphError::Serialization(e.to_string()))?;
        let text = match c.style {
            ApiStyle::Anthropic => v["content"].as_array().map(|a| {
                a.iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            }),
            ApiStyle::OpenAi => v["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.to_string()),
        };
        text.filter(|t| !t.is_empty())
            .ok_or_else(|| SimorphError::Serialization(format!("unexpected LLM response: {v}")))
    }
}

/// Extract the first JSON object of a model answer (tolerates code fences).
pub fn extract_json(answer: &str) -> Result<serde_json::Value> {
    let start = answer
        .find('{')
        .ok_or_else(|| SimorphError::Serialization("no JSON object in LLM answer".into()))?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in answer[start..].char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(serde_json::from_str(&answer[start..start + i + 1])?);
                }
            }
            _ => {}
        }
    }
    Err(SimorphError::Serialization(
        "unterminated JSON object in LLM answer".into(),
    ))
}

/// Draft proposed by an LLM (subset of `ProblemSpec`, all names are ports).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmDraft {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub knowns: Vec<Quantity>,
    #[serde(default)]
    pub design_variables: Vec<DesignVariable>,
    #[serde(default)]
    pub objectives: Vec<Objective>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    #[serde(default)]
    pub unknowns: Vec<Unknown>,
}

fn check_item(
    cat: &Catalog,
    amb: &mut Vec<Ambiguity>,
    mappings: &mut Vec<ConceptMapping>,
    name: &str,
    unit: &str,
    input: bool,
    role: MentionRole,
) -> bool {
    let Some(p) = cat.port(name, input) else {
        amb.push(Ambiguity {
            kind: AmbiguityKind::InvalidProposal,
            sentence: None,
            text: name.into(),
            message: format!(
                "proposition LLM rejetée : `{name}` n'est pas un port {} d'un composant disponible",
                if input { "d'entrée" } else { "de sortie" }
            ),
        });
        return false;
    };
    if let Err(e) = simorph_units::check_same_dimension(unit, &p.unit) {
        amb.push(Ambiguity {
            kind: AmbiguityKind::InvalidProposal,
            sentence: None,
            text: name.into(),
            message: format!("proposition LLM rejetée : unité incompatible ({e})"),
        });
        return false;
    }
    mappings.push(ConceptMapping {
        sentence: 0,
        phrase: name.into(),
        role,
        port: name.into(),
        port_unit: p.unit.clone(),
        components: p.components.clone(),
        confidence: 1.0,
        alternatives: vec![],
        source: "llm".into(),
    });
    true
}

/// Keep only the items whose names are catalogue ports with the right
/// role and a compatible unit; everything else becomes an ambiguity.
pub fn validate_against_catalog(
    text: &str,
    draft: LlmDraft,
    manifests: &[ComponentManifest],
) -> UnderstandingResult {
    let cat = Catalog::from_manifests(manifests);
    let mut amb = vec![];
    let mut mappings = vec![];
    let mut spec = ProblemSpec::new(
        if draft.title.is_empty() {
            "Problème (proposition LLM)"
        } else {
            &draft.title
        },
        text,
    );
    for k in draft.knowns {
        if check_item(
            &cat,
            &mut amb,
            &mut mappings,
            &k.name,
            &k.unit,
            true,
            MentionRole::Known,
        ) {
            spec.knowns.push(k);
        }
    }
    for d in draft.design_variables {
        if d.low > d.high {
            amb.push(Ambiguity {
                kind: AmbiguityKind::InvalidProposal,
                sentence: None,
                text: d.name.clone(),
                message: "proposition LLM rejetée : borne basse > borne haute".into(),
            });
            continue;
        }
        if check_item(
            &cat,
            &mut amb,
            &mut mappings,
            &d.name,
            &d.unit,
            true,
            MentionRole::DesignVariable,
        ) {
            spec.design_variables.push(d);
        }
    }
    for o in draft.objectives {
        if check_item(
            &cat,
            &mut amb,
            &mut mappings,
            &o.variable,
            &o.unit,
            false,
            MentionRole::Objective,
        ) {
            spec.objectives.push(o);
        }
    }
    for c in draft.constraints {
        if check_item(
            &cat,
            &mut amb,
            &mut mappings,
            &c.variable,
            &c.unit,
            false,
            MentionRole::Constraint,
        ) {
            spec.constraints.push(c);
        }
    }
    for u in draft.unknowns {
        if check_item(
            &cat,
            &mut amb,
            &mut mappings,
            &u.name,
            &u.unit,
            false,
            MentionRole::Unknown,
        ) {
            spec.unknowns.push(u);
        }
    }
    if let Err(e) = spec.validate_structure() {
        amb.push(Ambiguity {
            kind: AmbiguityKind::InvalidProposal,
            sentence: None,
            text: String::new(),
            message: format!("spécification proposée invalide : {e}"),
        });
    }
    UnderstandingResult {
        spec,
        mappings,
        ambiguities: amb,
        unsupported_phrases: vec![],
        quantities: vec![],
    }
}

/// LLM-backed interpreter: proposes, the deterministic validator decides.
pub struct LlmInterpreter {
    pub client: LlmClient,
}

impl LlmInterpreter {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            client: LlmClient::from_env()?,
        })
    }

    fn prompt(manifests: &[ComponentManifest]) -> String {
        let mut s = String::from(
            "You convert an engineering problem statement into JSON with keys title, knowns [{name,value,unit}], design_variables [{name,unit,low,high}], objectives [{name,direction (minimize|maximize|target),variable,unit}], constraints [{name,variable,kind (upper_bound|lower_bound|equality),bound,unit}], unknowns [{name,unit}]. Use ONLY these port names. Inputs (knowns, design_variables):\n",
        );
        let cat = Catalog::from_manifests(manifests);
        for p in cat.ports.iter().filter(|p| p.input) {
            s.push_str(&format!("- {} [{}]\n", p.name, p.unit));
        }
        s.push_str("Outputs (objectives, constraints, unknowns):\n");
        for p in cat.ports.iter().filter(|p| !p.input) {
            s.push_str(&format!("- {} [{}]\n", p.name, p.unit));
        }
        s.push_str("Answer with the JSON object only.");
        s
    }
}

impl crate::Interpreter for LlmInterpreter {
    fn name(&self) -> &str {
        "llm"
    }
    fn interpret(
        &self,
        text: &str,
        manifests: &[ComponentManifest],
    ) -> Result<UnderstandingResult> {
        let answer = self.client.complete(&Self::prompt(manifests), text)?;
        let v = extract_json(&answer)?;
        let draft: LlmDraft = serde_json::from_value(v)?;
        Ok(validate_against_catalog(text, draft, manifests))
    }
}
