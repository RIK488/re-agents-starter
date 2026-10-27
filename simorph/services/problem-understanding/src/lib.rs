//! # Problem understanding (subsystem 3)
//!
//! Turns a natural-language problem statement (French or English) into a
//! [`simorph_core::ProblemSpec`] *draft* plus an explicit list of
//! [`Ambiguity`]s.  The pipeline is deterministic and never silently
//! guesses: every quantity whose unit or concept cannot be resolved is
//! reported and left out of the specification.
//!
//! 1. sentence segmentation, tokenisation (French decimal comma, thousands
//!    separators, superscript units, `±`, ranges);
//! 2. quantity extraction with units validated by `simorph_units`;
//! 3. role detection with lexicons (known value, design variable,
//!    uncertainty, objective, constraint, unknown, budget);
//! 4. concept → component-port mapping using synonyms, weighted coverage of
//!    the words nearest to the quantity, unit compatibility and component
//!    coherence (ports of the same component support each other);
//! 5. assembly of the `ProblemSpec`, with output variables expressed in the
//!    unit the author first wrote.
//!
//! An optional [`llm::LlmInterpreter`] can *propose* a specification; its
//! proposal is always re-validated by [`llm::validate_against_catalog`]
//! (units, ports, roles) and can never bypass it.

pub mod catalog;
pub mod lexicon;
pub mod llm;
pub mod quantity;
pub mod text;
pub mod understand;

pub use catalog::Catalog;
pub use quantity::{extract, ExtractedQuantity, QKind};
pub use understand::*;

/// Interpreter interface shared by the deterministic pipeline and the
/// optional LLM proposer.
pub trait Interpreter: Send + Sync {
    fn name(&self) -> &str;
    fn interpret(
        &self,
        text: &str,
        manifests: &[simorph_core::ComponentManifest],
    ) -> simorph_core::Result<UnderstandingResult>;
}

/// The deterministic interpreter.
#[derive(Default)]
pub struct DeterministicInterpreter;

impl Interpreter for DeterministicInterpreter {
    fn name(&self) -> &str {
        "deterministic"
    }
    fn interpret(
        &self,
        text: &str,
        manifests: &[simorph_core::ComponentManifest],
    ) -> simorph_core::Result<UnderstandingResult> {
        Ok(understand(text, manifests))
    }
}

/// Extract quantities of a whole text (used for precision/recall evaluation).
pub fn extract_quantities(text: &str) -> Vec<ExtractedQuantity> {
    let lex = lexicon::Lexicon::load();
    let mut out = vec![];
    for (i, s) in text::sentences(text).iter().enumerate() {
        let toks = text::tokenize_sentence(s);
        let (qs, issues) = extract(&toks, i, &lex);
        out.extend(
            qs.into_iter()
                .filter(|q| !issues.iter().any(|x| x.token == q.end)),
        );
    }
    out
}
