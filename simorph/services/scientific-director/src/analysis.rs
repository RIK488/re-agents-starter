//! Problem analysis: the eleven questions of §2.

use crate::catalog::{compute_kind, ComponentCatalog, ComputeKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use std::collections::{BTreeMap, BTreeSet};

/// Epistemic / operational category of a quantity in the problem.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum QuantityCategory {
    /// Value supplied by the problem owner.
    Known,
    /// Nothing in the problem or the catalogue determines it.
    Unknown,
    /// A closed-form / exact component produces it from knowns.
    DirectlyComputable,
    /// A model must be assembled (chain of components or new descriptor).
    RequiresModelling,
    /// A simulation component produces it (result will be `Simulated`).
    Simulable,
    /// No component can produce it: real data are required.
    RequiresData,
    /// The owner asks for a mathematical demonstration.
    RequiresProof,
    /// Simulable but no empirical reference exists: must be verified experimentally.
    RequiresExperimentalVerification,
}

impl QuantityCategory {
    pub fn label_fr(self) -> &'static str {
        match self {
            QuantityCategory::Known => "connu",
            QuantityCategory::Unknown => "inconnu",
            QuantityCategory::DirectlyComputable => "calculable directement",
            QuantityCategory::RequiresModelling => "à modéliser",
            QuantityCategory::Simulable => "simulable",
            QuantityCategory::RequiresData => "données supplémentaires nécessaires",
            QuantityCategory::RequiresProof => "à démontrer mathématiquement",
            QuantityCategory::RequiresExperimentalVerification => "à vérifier expérimentalement",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuantityAssessment {
    pub name: String,
    pub unit: String,
    pub categories: Vec<QuantityCategory>,
    pub rationale: String,
    /// Components able to produce the quantity, best first.
    pub candidate_components: Vec<ComponentId>,
    /// Inputs those components need that are not known (drives "requires data").
    pub missing_inputs: Vec<String>,
}

/// A candidate solution path identified by the analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SolutionCandidate {
    pub name: String,
    pub component: ComponentId,
    pub produces: Vec<String>,
    /// Inputs the component needs (port names).
    pub requires: Vec<String>,
    /// Required inputs that are neither known nor design variables.
    pub missing_inputs: Vec<String>,
    pub compute_kind: String,
    pub expected_evidence: EvidenceLevel,
    pub resource_requirements: ResourceRequirements,
    pub assumptions: Vec<String>,
}

/// Objective comparison criterion fixed before any experiment runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComparisonCriterion {
    pub name: String,
    pub variable: String,
    pub direction: Direction,
    pub unit: String,
    pub weight: f64,
    pub hard_constraints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProblemAnalysis {
    pub problem_id: ProblemId,
    pub disciplines: Vec<String>,
    pub assessments: Vec<QuantityAssessment>,
    pub solution_candidates: Vec<SolutionCandidate>,
    pub comparison_criteria: Vec<ComparisonCriterion>,
    /// Unit problems found in the specification (objective/constraint units
    /// incompatible with producing ports).
    pub dimensional_issues: Vec<String>,
    /// Suggested improvements to models/tools/methods (§2 item 11).
    pub improvement_opportunities: Vec<String>,
}

impl ProblemAnalysis {
    pub fn by_category(&self) -> BTreeMap<QuantityCategory, Vec<String>> {
        let mut map: BTreeMap<QuantityCategory, Vec<String>> = BTreeMap::new();
        for a in &self.assessments {
            for c in &a.categories {
                map.entry(*c).or_default().push(a.name.clone());
            }
        }
        map
    }

    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        s.push_str("## Analyse du problème\n\n");
        s.push_str(&format!(
            "Disciplines : {}\n\n",
            self.disciplines.join(", ")
        ));
        for (cat, names) in self.by_category() {
            s.push_str(&format!(
                "- **{}** : {}\n",
                cat.label_fr(),
                names.join(", ")
            ));
        }
        if !self.dimensional_issues.is_empty() {
            s.push_str("\n### Incohérences dimensionnelles\n");
            for d in &self.dimensional_issues {
                s.push_str(&format!("- {d}\n"));
            }
        }
        s.push_str("\n### Solutions envisageables\n");
        for c in &self.solution_candidates {
            s.push_str(&format!(
                "- {} → {} ({}, niveau attendu : {})\n",
                c.name,
                c.produces.join(", "),
                c.compute_kind,
                c.expected_evidence.label_fr()
            ));
        }
        s.push_str("\n### Critères de comparaison (fixés avant exécution)\n");
        for c in &self.comparison_criteria {
            s.push_str(&format!(
                "- {} : {:?} `{}` [{}], poids {}\n",
                c.name, c.direction, c.variable, c.unit, c.weight
            ));
        }
        if !self.improvement_opportunities.is_empty() {
            s.push_str("\n### Améliorations possibles des modèles et méthodes\n");
            for i in &self.improvement_opportunities {
                s.push_str(&format!("- {i}\n"));
            }
        }
        s
    }
}

/// Whether `name` can be obtained from the knowns through a chain of
/// catalogue components (bounded depth to avoid cycles).
fn resolvable(
    name: &str,
    known: &BTreeSet<String>,
    catalog: &dyn ComponentCatalog,
    depth: usize,
) -> bool {
    if known.contains(name) {
        return true;
    }
    if depth > 8 {
        return false;
    }
    catalog.producers_of(name).iter().any(|m| {
        m.input_schema
            .iter()
            .all(|p| p.name != name && resolvable(&p.name, known, catalog, depth + 1))
    })
}

/// Analyse a problem against a catalogue.
pub fn analyse(problem: &ProblemSpec, catalog: &dyn ComponentCatalog) -> Result<ProblemAnalysis> {
    problem.validate_structure()?;
    let known: BTreeSet<String> = problem
        .knowns
        .iter()
        .map(|q| q.name.clone())
        .chain(problem.design_variables.iter().map(|d| d.name.clone()))
        .collect();
    let known_units: BTreeMap<String, String> = problem
        .knowns
        .iter()
        .map(|q| (q.name.clone(), q.unit.clone()))
        .chain(
            problem
                .design_variables
                .iter()
                .map(|d| (d.name.clone(), d.unit.clone())),
        )
        .collect();

    let mut assessments = vec![];
    let mut dimensional_issues = vec![];
    let mut candidates: Vec<SolutionCandidate> = vec![];
    let mut disciplines: BTreeSet<String> = problem.domains.iter().cloned().collect();
    let mut improvements: BTreeSet<String> = BTreeSet::new();

    for q in &problem.knowns {
        assessments.push(QuantityAssessment {
            name: q.name.clone(),
            unit: q.unit.clone(),
            categories: vec![QuantityCategory::Known],
            rationale: match &q.uncertainty {
                Uncertainty::Undeclared => {
                    "fourni par le propriétaire, incertitude non déclarée".into()
                }
                u => format!("fourni par le propriétaire, incertitude {u:?}"),
            },
            candidate_components: vec![],
            missing_inputs: vec![],
        });
    }

    // Targets: declared unknowns + objective/constraint variables not known.
    let mut targets: Vec<(String, String)> = problem
        .unknowns
        .iter()
        .map(|u| (u.name.clone(), u.unit.clone()))
        .collect();
    for o in &problem.objectives {
        if !known.contains(&o.variable) && !targets.iter().any(|t| t.0 == o.variable) {
            targets.push((o.variable.clone(), o.unit.clone()));
        }
    }
    for c in &problem.constraints {
        if !known.contains(&c.variable) && !targets.iter().any(|t| t.0 == c.variable) {
            targets.push((c.variable.clone(), c.unit.clone()));
        }
    }

    let wants_proof = |name: &str| {
        problem.hypotheses.iter().any(|h| {
            let s = h.statement.to_lowercase();
            s.contains(&name.to_lowercase())
                && (s.contains("prove")
                    || s.contains("démontr")
                    || s.contains("demontr")
                    || s.contains("proof"))
        })
    };

    for (name, unit) in &targets {
        let producers = catalog.producers_of(name);
        let mut cats = vec![];
        let mut rationale = String::new();
        let mut comps = vec![];
        let mut missing_all: BTreeSet<String> = BTreeSet::new();

        if producers.is_empty() {
            cats.push(QuantityCategory::Unknown);
            cats.push(QuantityCategory::RequiresModelling);
            rationale.push_str("aucun composant du catalogue ne produit cette grandeur : un modèle doit être construit ou intégré");
        } else {
            for m in &producers {
                // unit check against the producing port
                if let Some(port) = m.output_schema.iter().find(|p| &p.name == name) {
                    if simorph_units::check_same_dimension(&port.unit, unit).is_err() {
                        dimensional_issues.push(format!(
                            "`{name}` demandé en [{unit}] mais produit en [{}] par {}",
                            port.unit, m.name
                        ));
                        continue;
                    }
                }
                let missing: Vec<String> = m
                    .input_schema
                    .iter()
                    .filter(|p| !known.contains(&p.name))
                    .map(|p| p.name.clone())
                    .collect();
                // input unit check
                for p in &m.input_schema {
                    if let Some(ku) = known_units.get(&p.name) {
                        if simorph_units::check_same_dimension(ku, &p.unit).is_err() {
                            dimensional_issues.push(format!(
                                "entrée `{}` connue en [{ku}] mais attendue en [{}] par {}",
                                p.name, p.unit, m.name
                            ));
                        }
                    }
                }
                for d in &m.supported_domains {
                    disciplines.insert(d.clone());
                }
                let kind = compute_kind(m);
                let expected = match kind {
                    ComputeKind::Direct => EvidenceLevel::Computed,
                    ComputeKind::Simulation | ComputeKind::DataDriven | ComputeKind::Other => {
                        EvidenceLevel::Simulated
                    }
                    ComputeKind::Proof => EvidenceLevel::MathematicallyProven,
                };
                comps.push(m.id);
                candidates.push(SolutionCandidate {
                    name: m.name.clone(),
                    component: m.id,
                    produces: m.output_schema.iter().map(|p| p.name.clone()).collect(),
                    requires: m.input_schema.iter().map(|p| p.name.clone()).collect(),
                    missing_inputs: missing.clone(),
                    compute_kind: format!("{kind:?}"),
                    expected_evidence: expected,
                    resource_requirements: m.resource_requirements.clone(),
                    assumptions: m.assumptions.clone(),
                });
                if missing.is_empty() {
                    match kind {
                        ComputeKind::Direct => cats.push(QuantityCategory::DirectlyComputable),
                        ComputeKind::Proof => cats.push(QuantityCategory::RequiresProof),
                        _ => {
                            cats.push(QuantityCategory::Simulable);
                            if !catalog.has_empirical_reference(m.id) {
                                cats.push(QuantityCategory::RequiresExperimentalVerification);
                            }
                        }
                    }
                } else {
                    missing_all.extend(missing.iter().cloned());
                }
                if m.assumptions.is_empty() {
                    improvements.insert(format!(
                        "{} ne déclare aucune hypothèse : documenter son domaine de validité",
                        m.name
                    ));
                }
                if matches!(m.uncertainty_model, UncertaintyModel::None) {
                    improvements.insert(format!(
                        "{} ne quantifie pas son incertitude : ajouter un modèle d'incertitude",
                        m.name
                    ));
                }
            }
            if cats.is_empty() {
                // producers exist but inputs are missing: can the missing
                // inputs be produced recursively from the knowns?
                let unresolvable: Vec<String> = missing_all
                    .iter()
                    .filter(|m| !resolvable(m, &known, catalog, 0))
                    .cloned()
                    .collect();
                if unresolvable.is_empty() {
                    cats.push(QuantityCategory::RequiresModelling);
                    rationale.push_str("les entrées manquantes peuvent être produites par d'autres composants : chaînage de modèles requis");
                } else {
                    cats.push(QuantityCategory::RequiresData);
                    rationale.push_str(&format!(
                        "entrées sans valeur ni chaîne de production depuis les connus : {}",
                        unresolvable.join(", ")
                    ));
                }
            } else {
                rationale.push_str(&format!("{} composant(s) compatible(s)", comps.len()));
            }
        }
        if wants_proof(name) && !cats.contains(&QuantityCategory::RequiresProof) {
            cats.push(QuantityCategory::RequiresProof);
        }
        cats.sort();
        cats.dedup();
        assessments.push(QuantityAssessment {
            name: name.clone(),
            unit: unit.clone(),
            categories: cats,
            rationale,
            candidate_components: comps,
            missing_inputs: missing_all.into_iter().collect(),
        });
    }

    let comparison_criteria = problem
        .objectives
        .iter()
        .map(|o| ComparisonCriterion {
            name: o.name.clone(),
            variable: o.variable.clone(),
            direction: o.direction,
            unit: o.unit.clone(),
            weight: o.weight,
            hard_constraints: problem
                .constraints
                .iter()
                .filter(|c| c.hard)
                .map(|c| c.name.clone())
                .collect(),
        })
        .collect();

    candidates.sort_by(|a, b| a.name.cmp(&b.name));
    candidates.dedup_by(|a, b| a.component == b.component);

    Ok(ProblemAnalysis {
        problem_id: problem.id,
        disciplines: disciplines.into_iter().collect(),
        assessments,
        solution_candidates: candidates,
        comparison_criteria,
        dimensional_issues,
        improvement_opportunities: improvements.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::StaticCatalog;

    fn catalog() -> StaticCatalog {
        let direct = ComponentManifest::in_process("beam", "1", "t")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_domains(&["civil"])
            .with_inputs(vec![Port::scalar("length", "m"), Port::scalar("load", "N")])
            .with_outputs(vec![Port::scalar("deflection", "m")]);
        let sim = ComponentManifest::in_process("thermal", "1", "t")
            .with_capabilities(vec![Capability::OdeIntegration])
            .with_domains(&["thermal"])
            .with_inputs(vec![Port::scalar("heat", "W")])
            .with_outputs(vec![Port::scalar("temperature", "K")]);
        let heat = ComponentManifest::in_process("heater", "1", "t")
            .with_capabilities(vec![Capability::AlgebraicSolver])
            .with_inputs(vec![
                Port::scalar("voltage", "V"),
                Port::scalar("current", "A"),
            ])
            .with_outputs(vec![Port::scalar("heat", "W")]);
        StaticCatalog::new(vec![direct, sim, heat])
    }

    #[test]
    fn classifies_each_category() {
        let mut p = ProblemSpec::new("t", "d");
        p.knowns.push(Quantity::new("length", 2.0, "m"));
        p.knowns.push(Quantity::new("load", 100.0, "N"));
        p.knowns.push(Quantity::new("voltage", 12.0, "V"));
        p.unknowns.push(Unknown {
            name: "deflection".into(),
            unit: "mm".into(),
            prior: None,
            description: None,
        });
        p.unknowns.push(Unknown {
            name: "temperature".into(),
            unit: "degC".into(),
            prior: None,
            description: None,
        });
        p.unknowns.push(Unknown {
            name: "lifetime".into(),
            unit: "yr".into(),
            prior: None,
            description: None,
        });
        p.unknowns.push(Unknown {
            name: "heat".into(),
            unit: "W".into(),
            prior: None,
            description: None,
        });
        let a = analyse(&p, &catalog()).unwrap();
        let by = a.by_category();
        assert!(by[&QuantityCategory::Known].contains(&"length".to_string()));
        assert!(by[&QuantityCategory::DirectlyComputable].contains(&"deflection".to_string()));
        // temperature needs `heat`, which heater can produce from voltage+current, but current is unknown → requires data
        assert!(by[&QuantityCategory::RequiresData].contains(&"temperature".to_string()));
        assert!(by[&QuantityCategory::RequiresModelling].contains(&"lifetime".to_string()));
        assert!(by[&QuantityCategory::Unknown].contains(&"lifetime".to_string()));
        assert!(by[&QuantityCategory::RequiresData].contains(&"heat".to_string()));
        assert!(a.disciplines.contains(&"civil".to_string()));
        assert!(a.dimensional_issues.is_empty());
        assert!(!a.to_markdown().is_empty());
    }

    #[test]
    fn simulable_requires_experimental_verification_without_empirical_reference() {
        let mut p = ProblemSpec::new("t", "d");
        p.knowns.push(Quantity::new("heat", 5.0, "W"));
        p.unknowns.push(Unknown {
            name: "temperature".into(),
            unit: "K".into(),
            prior: None,
            description: None,
        });
        let a = analyse(&p, &catalog()).unwrap();
        let by = a.by_category();
        assert!(by[&QuantityCategory::Simulable].contains(&"temperature".to_string()));
        assert!(by[&QuantityCategory::RequiresExperimentalVerification]
            .contains(&"temperature".to_string()));
    }

    #[test]
    fn dimensional_issue_is_reported() {
        let mut p = ProblemSpec::new("t", "d");
        p.knowns.push(Quantity::new("length", 2.0, "m"));
        p.knowns.push(Quantity::new("load", 100.0, "N"));
        p.unknowns.push(Unknown {
            name: "deflection".into(),
            unit: "s".into(),
            prior: None,
            description: None,
        });
        let a = analyse(&p, &catalog()).unwrap();
        assert_eq!(a.dimensional_issues.len(), 1);
    }
}
