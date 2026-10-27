//! The ten-step pipeline.

use crate::python::generate_python_script;
use crate::report::*;
use crate::wrapper::FactoryComponent;
use indexmap::IndexMap;
use serde_json::json;
use sha2::{Digest, Sha256};
use simorph_adapter_python_bridge::{
    bundled_python_dir, python_with_numpy_available, PythonComponent,
};
use simorph_core::*;
use simorph_model_registry::Registry;
use simorph_plugin_sdk::{ComponentBuilder, ComponentSet, SharedComponent};
use simorph_provenance::{Ledger, ProvenanceKind, RecordId};
use simorph_security::SandboxPolicy;
use simorph_simulation::{
    model::numerical_jacobian, simulate, DynamicalModel, OdeComponent, Solver, SolverOptions,
};
use simorph_svc_model_factory::{check_expression_unit, symbols, ExpressionModel};
use simorph_validation::{
    checks, classify, evaluate, CheckResult, ReferenceOrigin, ValidationTarget,
};
use std::path::PathBuf;
use std::sync::Arc;

/// Equations supplied for generation.
#[derive(Debug, Clone, Default)]
pub struct EquationSpec {
    pub equations: Vec<String>,
    pub variables: Vec<Port>,
    pub parameters: Vec<Quantity>,
    pub reference_cases: Vec<ReferenceCase>,
    pub domains: Vec<String>,
}

/// A declared algebraic relation used by step 5 to derive a missing output.
#[derive(Debug, Clone)]
pub struct Derivation {
    pub output: String,
    pub unit: String,
    pub expression: String,
}

#[derive(Debug, Clone)]
pub struct SimulatorRequest {
    pub name: String,
    pub required_outputs: Vec<String>,
    /// Known inputs with values and units (problem units).
    pub knowns: Vec<Quantity>,
    pub domains: Vec<String>,
    /// 0 = cheapest, 1 = full fidelity (tolerances 1e-6/1e-9 × 10^(3(1−f))).
    pub fidelity: f64,
    pub target_relative_error: f64,
    /// Integration horizon used for configuration and method comparison (s).
    pub horizon: f64,
    pub equations: Option<EquationSpec>,
    /// Force the Python generation path.
    pub prefer_python: bool,
    pub derivations: Vec<Derivation>,
}

impl SimulatorRequest {
    pub fn new(name: &str, required_outputs: &[&str]) -> Self {
        Self {
            name: name.into(),
            required_outputs: required_outputs.iter().map(|s| s.to_string()).collect(),
            knowns: vec![],
            domains: vec![],
            fidelity: 1.0,
            target_relative_error: 1e-6,
            horizon: 1.0,
            equations: None,
            prefer_python: false,
            derivations: vec![],
        }
    }
}

#[allow(clippy::large_enum_variant)]
enum Source {
    Existing(ComponentManifest),
    Equations(Arc<ExpressionModel>),
    None,
}

pub struct SimulatorFactory {
    pub registry: Registry,
    pub components: ComponentSet,
    pub ledger: Ledger,
    /// Directory where generated scripts are written.
    pub workdir: PathBuf,
    /// Directory holding the `simorph_adapter` package.
    pub python_adapter_dir: PathBuf,
}

struct Ctx {
    steps: Vec<StepReport>,
    source: Source,
    component: Option<SharedComponent>,
    opts: Option<SolverOptions>,
    stiff: bool,
    comparison: Option<MethodComparison>,
    validation: Option<simorph_validation::ValidationRecord>,
    script: Option<(String, String)>,
    generated: bool,
    python: bool,
    final_manifest: Option<ComponentManifest>,
    final_name: Option<String>,
    extra_assumptions: Vec<String>,
}

const STEP_NAMES: [&str; 10] = [
    "recherche d'un simulateur",
    "vérification de compatibilité",
    "configuration",
    "intégration",
    "complément",
    "génération",
    "tests de validité",
    "comparaison des méthodes numériques",
    "amélioration",
    "documentation des limites",
];

impl SimulatorFactory {
    pub fn new(registry: Registry, components: ComponentSet, ledger: Ledger) -> Self {
        let workdir = std::env::temp_dir().join(format!("simorph-simfactory-{}", uuid_like()));
        Self {
            registry,
            components,
            ledger,
            workdir,
            python_adapter_dir: bundled_python_dir(),
        }
    }

    fn record(&self, ctx: &mut Ctx, step: u8, outcome: StepOutcome, details: Vec<String>) {
        let payload = json!({"step": step, "name": STEP_NAMES[step as usize - 1], "outcome": outcome, "details": details});
        let kind = match outcome {
            StepOutcome::Failed { .. } => ProvenanceKind::Failure,
            _ => ProvenanceKind::Decision,
        };
        let prov: Option<RecordId> = self
            .ledger
            .append(
                kind,
                format!(
                    "simulator-factory step {step}: {}",
                    STEP_NAMES[step as usize - 1]
                ),
                payload,
                &[],
            )
            .ok();
        ctx.steps.push(StepReport {
            step,
            name: STEP_NAMES[step as usize - 1].into(),
            outcome,
            details,
            provenance: prov,
        });
    }

    fn skip(&self, ctx: &mut Ctx, step: u8, reason: &str) {
        self.record(
            ctx,
            step,
            StepOutcome::Skipped {
                reason: reason.into(),
            },
            vec![],
        );
    }

    /// Fidelity → tolerances.
    fn tolerances(fidelity: f64) -> (f64, f64) {
        let loosen = 10f64.powf(3.0 * (1.0 - fidelity.clamp(0.0, 1.0)));
        (1e-6 * loosen, 1e-9 * loosen)
    }

    /// Known value converted to the unit of a port (if the port exists).
    fn known_in_port_unit(
        req: &SimulatorRequest,
        m: &ComponentManifest,
        name: &str,
    ) -> Result<Option<f64>> {
        let Some(q) = req.knowns.iter().find(|q| q.name == name) else {
            return Ok(None);
        };
        let port = m.input_schema.iter().find(|p| p.name == name);
        match port {
            Some(p) => Ok(Some(simorph_units::convert(q.value, &q.unit, &p.unit)?)),
            None => Ok(Some(q.value)),
        }
    }

    /// Nominal request: every input port filled from the knowns (converted),
    /// `t_end` from the horizon when not known.
    fn nominal_request(req: &SimulatorRequest, m: &ComponentManifest) -> Result<TaskRequest> {
        let mut r = TaskRequest::new(m.id).fidelity(req.fidelity);
        for p in &m.input_schema {
            if let Some(v) = Self::known_in_port_unit(req, m, &p.name)? {
                r.inputs.insert(p.name.clone(), json!(v));
            } else if p.name == "t_end" {
                r.inputs.insert("t_end".into(), json!(req.horizon));
            }
        }
        Ok(r)
    }

    /// Run the pipeline.
    pub fn run(&self, req: &SimulatorRequest, ctl: &ExecutionControl) -> Result<GenerationReport> {
        let mut ctx = Ctx {
            steps: vec![],
            source: Source::None,
            component: None,
            opts: None,
            stiff: false,
            comparison: None,
            validation: None,
            script: None,
            generated: false,
            python: false,
            final_manifest: None,
            final_name: None,
            extra_assumptions: vec![],
        };
        let _ = self.ledger.append(ProvenanceKind::Problem, format!("simulator request: {}", req.name), json!({"required_outputs": req.required_outputs, "domains": req.domains, "horizon": req.horizon, "fidelity": req.fidelity}), &[]);
        self.step1_search(req, &mut ctx);
        self.step2_compatibility(req, &mut ctx);
        self.step3_configure(req, &mut ctx, ctl);
        self.step4_integrate(req, &mut ctx);
        self.step5_complete(req, &mut ctx);
        self.step6_generate(req, &mut ctx, ctl);
        self.step7_test(req, &mut ctx, ctl);
        self.step8_compare(req, &mut ctx, ctl);
        self.step9_improve(req, &mut ctx);
        let doc = self.step10_document(req, &mut ctx);
        Ok(GenerationReport {
            request_name: req.name.clone(),
            steps: ctx.steps,
            component_name: ctx.final_name,
            manifest: ctx.final_manifest,
            method_comparison: ctx.comparison,
            validation: ctx.validation,
            generated_script: ctx.script,
            documentation: doc,
        })
    }

    // ------------------------------------------------------------------ 1
    fn step1_search(&self, req: &SimulatorRequest, ctx: &mut Ctx) {
        let mut details = vec![];
        let mut full: Vec<ComponentManifest> = self
            .registry
            .manifests()
            .filter(|m| {
                req.required_outputs
                    .iter()
                    .all(|o| m.output_schema.iter().any(|p| &p.name == o))
            })
            .cloned()
            .collect();
        // prefer domain matches, then components with reference cases
        full.sort_by_key(|m| {
            let dom = if req.domains.iter().any(|d| m.supported_domains.contains(d)) {
                0
            } else {
                1
            };
            let refs = if m.validation_protocol.reference_cases.is_empty() {
                1
            } else {
                0
            };
            (dom, refs)
        });
        let partial: Vec<&ComponentManifest> = self
            .registry
            .manifests()
            .filter(|m| {
                req.required_outputs
                    .iter()
                    .any(|o| m.output_schema.iter().any(|p| &p.name == o))
            })
            .filter(|m| !full.iter().any(|f| f.id == m.id))
            .collect();
        details.push(format!(
            "{} composant(s) produisant toutes les sorties requises : {}",
            full.len(),
            full.iter()
                .map(|m| m.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if !partial.is_empty() {
            details.push(format!(
                "{} composant(s) partiels : {}",
                partial.len(),
                partial
                    .iter()
                    .map(|m| m.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if let Some(m) = full.first() {
            ctx.source = Source::Existing(m.clone());
            details.push(format!(
                "candidat retenu : {} v{} ({})",
                m.name, m.version, m.provider
            ));
        } else if let Some(m) = partial.first().filter(|_| !req.derivations.is_empty()) {
            // partial producer + derivations: step 5 may complete it
            ctx.source = Source::Existing((*m).clone());
            details.push(format!(
                "candidat partiel retenu pour complément : {}",
                m.name
            ));
        } else if let Some(eq) = &req.equations {
            match build_model(req, eq) {
                Ok(m) => {
                    details.push(format!("aucun composant enregistré ; modèle construit à partir de {} équation(s) fournies ({} contrôle(s) dimensionnel(s) réussi(s))", eq.equations.len(), m.dimensional_report.len()));
                    ctx.source = Source::Equations(Arc::new(m));
                }
                Err(e) => {
                    self.record(
                        ctx,
                        1,
                        StepOutcome::Failed {
                            reason: format!("équations fournies invalides : {e}"),
                        },
                        details,
                    );
                    return;
                }
            }
        } else {
            details.push("aucun composant enregistré ne produit les sorties requises et aucune équation n'a été fournie".into());
        }
        self.record(ctx, 1, StepOutcome::Done, details);
    }

    // ------------------------------------------------------------------ 2
    fn step2_compatibility(&self, req: &SimulatorRequest, ctx: &mut Ctx) {
        let mut details = vec![];
        let mut problems = vec![];
        match &ctx.source {
            Source::None => {
                self.skip(ctx, 2, "aucun candidat");
                return;
            }
            Source::Existing(m) => {
                for p in &m.input_schema {
                    match req.knowns.iter().find(|q| q.name == p.name) {
                        Some(q) => {
                            match simorph_units::convert(q.value, &q.unit, &p.unit) {
                                Ok(v) => {
                                    if let Some((lo, hi)) = p.range {
                                        if v < lo || v > hi {
                                            problems.push(format!(
                                                "`{}` = {v} {} hors plage du port [{lo}, {hi}]",
                                                p.name, p.unit
                                            ));
                                        }
                                    }
                                    if let Some((lo, hi)) = m.validity_domain.ranges.get(&p.name) {
                                        if v < *lo || v > *hi {
                                            problems.push(format!("`{}` = {v} {} hors domaine de validité [{lo}, {hi}]", p.name, p.unit));
                                        }
                                    }
                                    details.push(format!(
                                        "{} : {} {} → {v:.6e} {}",
                                        p.name, q.value, q.unit, p.unit
                                    ));
                                }
                                Err(e) => problems.push(format!("`{}` : {e}", p.name)),
                            }
                        }
                        None if p.name == "t_end" => {
                            details.push(format!("t_end pris égal à l'horizon {} s", req.horizon))
                        }
                        None => problems.push(format!("entrée `{}` [{}] inconnue", p.name, p.unit)),
                    }
                }
                let missing: Vec<&String> = req
                    .required_outputs
                    .iter()
                    .filter(|o| !m.output_schema.iter().any(|p| &&p.name == o))
                    .collect();
                if !missing.is_empty() {
                    details.push(format!(
                        "sorties non produites par le candidat (à dériver à l'étape 5) : {}",
                        missing
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
            Source::Equations(model) => {
                let produced: Vec<String> = model
                    .state_names()
                    .iter()
                    .map(|(s, _)| format!("final_{s}"))
                    .chain(model.output_names().iter().map(|(o, _)| o.clone()))
                    .collect();
                for o in &req.required_outputs {
                    if !produced.contains(o) {
                        problems.push(format!(
                            "les équations ne produisent pas `{o}` (sorties : {})",
                            produced.join(", ")
                        ));
                    }
                }
                for (n, u) in model.parameter_names() {
                    if let Some(q) = req.knowns.iter().find(|q| q.name == n) {
                        if let Err(e) = simorph_units::check_same_dimension(&q.unit, &u) {
                            problems.push(format!("paramètre `{n}` : {e}"));
                        }
                    }
                }
                details.push(
                    "paramètres sans valeur connue utilisent la valeur par défaut déclarée".into(),
                );
            }
        }
        if problems.is_empty() {
            self.record(ctx, 2, StepOutcome::Done, details);
        } else {
            // An incompatible existing candidate falls back to equations when available.
            let fallback = matches!(ctx.source, Source::Existing(_)) && req.equations.is_some();
            if fallback {
                details.push(
                    "candidat incompatible : bascule vers la génération à partir des équations"
                        .into(),
                );
                let eq = req.equations.as_ref().unwrap();
                match build_model(req, eq) {
                    Ok(m) => ctx.source = Source::Equations(Arc::new(m)),
                    Err(e) => problems.push(format!("équations invalides : {e}")),
                }
            }
            if fallback && matches!(ctx.source, Source::Equations(_)) {
                details.extend(problems);
                self.record(ctx, 2, StepOutcome::Done, details);
            } else {
                ctx.source = Source::None;
                self.record(
                    ctx,
                    2,
                    StepOutcome::Failed {
                        reason: problems.join(" ; "),
                    },
                    details,
                );
            }
        }
    }

    // ------------------------------------------------------------------ 3
    fn step3_configure(&self, req: &SimulatorRequest, ctx: &mut Ctx, ctl: &ExecutionControl) {
        let (rtol, atol) = Self::tolerances(req.fidelity);
        let mut details = vec![format!(
            "fidélité {} → rtol {rtol:.1e}, atol {atol:.1e}",
            req.fidelity
        )];
        let mut opts = SolverOptions::new(Solver::Rk45).with_tolerances(rtol, atol);
        let mut stiff = false;
        match &ctx.source {
            Source::None => {
                self.skip(ctx, 3, "aucun candidat");
                return;
            }
            Source::Equations(model) => {
                // Jacobian infinity-norm × horizon: explicit stability cost estimate.
                let mut params = model.default_params();
                for (n, u) in model.parameter_names() {
                    if let Some(q) = req.knowns.iter().find(|q| q.name == n) {
                        if let Ok(v) = simorph_units::convert(q.value, &q.unit, &u) {
                            params.insert(n, v);
                        }
                    }
                }
                let x0 = model.initial_state(&params);
                let n = x0.len();
                let mut jac = vec![0.0; n * n];
                let norm = match numerical_jacobian(model.as_ref(), 0.0, &x0, &params, &mut jac) {
                    Ok(_) => (0..n)
                        .map(|i| (0..n).map(|j| jac[i * n + j].abs()).sum::<f64>())
                        .fold(0.0, f64::max),
                    Err(e) => {
                        details.push(format!("jacobien non évaluable : {e}"));
                        0.0
                    }
                };
                let product = norm * req.horizon;
                details.push(format!(
                    "‖J‖∞ ≈ {norm:.3e} s⁻¹, ‖J‖∞·horizon = {product:.3e}"
                ));
                let mut trial = opts.clone();
                trial.max_steps = 20_000;
                let trial_res = simulate(
                    model.as_ref(),
                    &params,
                    None,
                    (0.0, req.horizon),
                    &trial,
                    ctl,
                );
                // Stability-limited stepping: the mean explicit step sits at the
                // Dormand–Prince stability boundary (‖J‖·h ≈ 3) instead of being
                // set by accuracy (‖J‖·h ≪ 1).
                let (jh, trial_detail) = match &trial_res {
                    Ok(t) => {
                        let h = req.horizon / t.stats.steps.max(1) as f64;
                        (
                            norm * h,
                            format!(
                                "essai RK45 : {} pas acceptés, {} rejetés, ‖J‖∞·h_moyen = {:.2}",
                                t.stats.steps,
                                t.stats.rejected_steps,
                                norm * h
                            ),
                        )
                    }
                    Err(e) => (f64::INFINITY, format!("essai RK45 échoué : {e}")),
                };
                details.push(trial_detail);
                stiff = trial_res.is_err() || (jh > 1.5 && product > 1e3);
                if stiff {
                    opts = SolverOptions::new(Solver::Trapezoidal)
                        .with_tolerances(rtol, atol)
                        .with_dt((req.horizon / 400.0).max(1e-12));
                    details.push("système raide : méthode implicite (trapèze) retenue".into());
                } else {
                    details.push("système non raide : RK45 adaptatif retenu".into());
                }
            }
            Source::Existing(m) => {
                if !m.has_capability(&Capability::OdeIntegration) {
                    details.push("composant algébrique : pas de solveur à configurer".into());
                } else if let Some(c) = self.components.get_by_id(m.id) {
                    match Self::nominal_request(req, m) {
                        Ok(r) => {
                            let r = r.config(json!({"solver": "rk45", "rtol": rtol, "atol": atol}));
                            match run_component(c.as_ref(), &r, ctl) {
                                Ok(res) => {
                                    let steps =
                                        res.metrics.get("steps").copied().unwrap_or(1.0).max(1.0);
                                    let rej =
                                        res.metrics.get("rejected_steps").copied().unwrap_or(0.0);
                                    details
                                        .push(format!("essai RK45 : {steps} pas, {rej} rejetés"));
                                    if rej / steps > 0.5 {
                                        stiff = true;
                                    }
                                }
                                Err(SimorphError::Numerical(e)) => {
                                    details.push(format!("essai RK45 échoué numériquement ({e})"));
                                    stiff = true;
                                }
                                Err(e) => details.push(format!("essai RK45 : {e}")),
                            }
                        }
                        Err(e) => details.push(format!("requête nominale impossible : {e}")),
                    }
                    if stiff {
                        opts = SolverOptions::new(Solver::Trapezoidal)
                            .with_tolerances(rtol, atol)
                            .with_dt((req.horizon / 400.0).max(1e-12));
                        details.push("système raide : méthode implicite (trapèze) retenue".into());
                    } else {
                        details.push("système non raide : RK45 adaptatif retenu".into());
                    }
                } else {
                    details.push("instance exécutable absente : configuration différée".into());
                }
            }
        }
        ctx.opts = Some(opts);
        ctx.stiff = stiff;
        self.record(ctx, 3, StepOutcome::Done, details);
    }

    // ------------------------------------------------------------------ 4
    fn step4_integrate(&self, req: &SimulatorRequest, ctx: &mut Ctx) {
        let existing = match &ctx.source {
            Source::Existing(m) => Some(m.clone()),
            _ => None,
        };
        if let Some(m) = existing {
            match self.components.get_by_id(m.id) {
                Some(c) => {
                    let details = vec![format!(
                        "composant `{}` disponible en processus ({:?})",
                        m.name, m.execution_interface
                    )];
                    ctx.component = Some(c);
                    self.record(ctx, 4, StepOutcome::Done, details);
                }
                None => {
                    ctx.source = Source::None;
                    self.record(ctx, 4, StepOutcome::Failed { reason: format!("le manifeste `{}` est enregistré sans instance exécutable dans le jeu de composants", m.name) }, vec![]);
                }
            }
            return;
        }
        match &ctx.source {
            Source::None => self.skip(ctx, 4, "aucun candidat"),
            Source::Existing(_) => unreachable!(),
            Source::Equations(model) => {
                if req.prefer_python {
                    self.skip(
                        ctx,
                        4,
                        "intégration réalisée à l'étape 6 via le pont Python",
                    );
                    return;
                }
                let opts = ctx.opts.clone().unwrap_or_default();
                let comp = OdeComponent::with_defaults(
                    model.clone(),
                    "0.1.0",
                    "simorph-simulator-factory",
                    opts.clone(),
                );
                let details = vec![format!(
                    "modèle d'équations encapsulé en OdeComponent (solveur par défaut {:?})",
                    opts.solver
                )];
                ctx.component = Some(Arc::new(comp));
                ctx.generated = true;
                self.record(ctx, 4, StepOutcome::Done, details);
            }
        }
    }

    // ------------------------------------------------------------------ 5
    fn step5_complete(&self, req: &SimulatorRequest, ctx: &mut Ctx) {
        let Some(base) = ctx.component.clone() else {
            self.skip(ctx, 5, "aucun composant à compléter");
            return;
        };
        let m = base.manifest().clone();
        let missing: Vec<String> = req
            .required_outputs
            .iter()
            .filter(|o| !m.output_schema.iter().any(|p| &&p.name == o))
            .cloned()
            .collect();
        if missing.is_empty() {
            self.skip(ctx, 5, "toutes les sorties requises sont produites");
            return;
        }
        let mut units: IndexMap<String, String> = m.units.clone();
        let mut derived: Vec<Derivation> = vec![];
        let mut details = vec![];
        let mut problems = vec![];
        for o in &missing {
            match req.derivations.iter().find(|d| &d.output == o) {
                None => problems.push(format!(
                    "`{o}` n'est pas produit et aucune relation de dérivation n'est déclarée"
                )),
                Some(d) => {
                    let syms = match symbols(&d.expression) {
                        Ok(s) => s,
                        Err(e) => {
                            problems.push(format!("`{o}` : {e}"));
                            continue;
                        }
                    };
                    if let Some(u) = syms.iter().find(|s| !units.contains_key(*s)) {
                        problems.push(format!("`{o}` = {} utilise `{u}` qui n'est ni une entrée ni une sortie du composant", d.expression));
                        continue;
                    }
                    match check_expression_unit(&d.expression, &d.unit, &units) {
                        Ok(()) => {
                            details.push(format!(
                                "`{o}` [{}] = {} (cohérent dimensionnellement)",
                                d.unit, d.expression
                            ));
                            units.insert(o.clone(), d.unit.clone());
                            derived.push(d.clone());
                        }
                        Err(e) => problems.push(format!("`{o}` : {e}")),
                    }
                }
            }
        }
        if !problems.is_empty() {
            self.record(
                ctx,
                5,
                StepOutcome::Failed {
                    reason: problems.join(" ; "),
                },
                details,
            );
            ctx.component = None;
            ctx.source = Source::None;
            return;
        }
        let name = format!("{}+derived", m.name);
        let mut b = ComponentBuilder::new(&name, &m.version, "simorph-simulator-factory")
            .evidence(EvidenceLevel::Simulated);
        for c in &m.capabilities {
            b = b.capability(c.clone());
        }
        for d in &m.supported_domains {
            b = b.domain(d);
        }
        for p in &m.input_schema {
            b = b.input(p.clone());
        }
        for p in &m.output_schema {
            b = b.output(p.clone());
        }
        for d in &derived {
            b = b.output(Port::scalar(&d.output, &d.unit));
            b = b.assumption(&format!(
                "derived output `{}` = {} (declared algebraic relation)",
                d.output, d.expression
            ));
        }
        for a in &m.assumptions {
            b = b.assumption(a);
        }
        let inner = base.clone();
        let inner_id = m.id;
        let derived_c = derived.clone();
        let built = b.build_raw(move |req, ctl| {
            let mut r = req.clone();
            r.component = inner_id;
            let mut res = inner.execute(&r, ctl)?;
            let mut vars = checks::result_variables(&res);
            for (k, v) in &r.inputs {
                if let Some(x) = v.as_f64() {
                    vars.entry(k.clone()).or_insert(x);
                }
            }
            for d in &derived_c {
                let v = evaluate(&d.expression, &vars)?.as_f64()?;
                vars.insert(d.output.clone(), v);
                res.outputs.insert(d.output.clone(), json!(v));
            }
            res.component = req.component;
            Ok(res)
        });
        match built {
            Ok(c) => {
                details.push(format!("composant complété `{name}`"));
                ctx.component = Some(Arc::new(c));
                self.record(ctx, 5, StepOutcome::Done, details);
            }
            Err(e) => self.record(
                ctx,
                5,
                StepOutcome::Failed {
                    reason: e.to_string(),
                },
                details,
            ),
        }
    }

    // ------------------------------------------------------------------ 6
    fn step6_generate(&self, req: &SimulatorRequest, ctx: &mut Ctx, ctl: &ExecutionControl) {
        match (&ctx.source, ctx.component.is_some()) {
            (Source::Existing(m), true) => {
                let reason = format!(
                    "composant existant `{}` : aucune génération nécessaire",
                    m.name
                );
                self.skip(ctx, 6, &reason);
            }
            (Source::Equations(model), has) => {
                if !req.prefer_python && has {
                    let details = vec![
                        format!(
                            "composant généré en processus à partir de {} équation(s) : {}",
                            model.equations().len(),
                            model
                                .equations()
                                .iter()
                                .map(|(l, r)| format!("{l} = {r}"))
                                .collect::<Vec<_>>()
                                .join(" ; ")
                        ),
                        "niveau de preuve maximal : résultat simulé".into(),
                    ];
                    let _ = self.ledger.append(ProvenanceKind::Component, format!("generated component {}", req.name), json!({"equations": model.equations(), "generated_by": "simorph-simulator-factory"}), &[]);
                    self.record(ctx, 6, StepOutcome::Done, details);
                    return;
                }
                // Python path
                let eq = req.equations.as_ref().expect("equations present");
                if let Err(reason) = python_with_numpy_available() {
                    self.record(
                        ctx,
                        6,
                        StepOutcome::Failed {
                            reason: format!("génération Python impossible : {reason}"),
                        },
                        vec![],
                    );
                    ctx.source = Source::None;
                    return;
                }
                let adapter_dir = self
                    .python_adapter_dir
                    .canonicalize()
                    .unwrap_or(self.python_adapter_dir.clone());
                let script = match generate_python_script(
                    &req.name,
                    &adapter_dir.to_string_lossy(),
                    &eq.equations.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                    &eq.variables,
                    &eq.parameters,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        self.record(
                            ctx,
                            6,
                            StepOutcome::Failed {
                                reason: format!("génération du script : {e}"),
                            },
                            vec![],
                        );
                        ctx.source = Source::None;
                        return;
                    }
                };
                let sha = hex::encode(Sha256::digest(script.as_bytes()));
                if let Err(e) = std::fs::create_dir_all(&self.workdir) {
                    self.record(
                        ctx,
                        6,
                        StepOutcome::Failed {
                            reason: format!("répertoire de travail : {e}"),
                        },
                        vec![],
                    );
                    ctx.source = Source::None;
                    return;
                }
                let path = self
                    .workdir
                    .join(format!("{}_{}.py", sanitize(&req.name), &sha[..12]));
                if let Err(e) = std::fs::write(&path, &script) {
                    self.record(
                        ctx,
                        6,
                        StepOutcome::Failed {
                            reason: format!("écriture du script : {e}"),
                        },
                        vec![],
                    );
                    ctx.source = Source::None;
                    return;
                }
                let prov = self.ledger.append(ProvenanceKind::Component, format!("generated python component {}", req.name), json!({"path": path.to_string_lossy(), "sha256": sha, "script": script, "generated_by": "simorph-simulator-factory"}), &[]).ok();
                let policy = SandboxPolicy::strict()
                    .with_timeout(60.0)
                    .with_working_dir(&self.workdir);
                match PythonComponent::from_script(&path, policy) {
                    Ok(c) => {
                        let details = vec![
                            format!("script Python généré : {} (sha256 {})", path.display(), sha),
                            "exécuté en sous-processus isolé (python3 -I, politique stricte)"
                                .into(),
                            "niveau de preuve maximal : résultat simulé".into(),
                        ];
                        ctx.component = Some(Arc::new(c));
                        ctx.generated = true;
                        ctx.python = true;
                        ctx.script = Some((path.to_string_lossy().to_string(), sha));
                        let mut s = StepReport {
                            step: 6,
                            name: STEP_NAMES[5].into(),
                            outcome: StepOutcome::Done,
                            details,
                            provenance: prov,
                        };
                        if ctl.check().is_err() {
                            s.outcome = StepOutcome::Failed {
                                reason: "annulé".into(),
                            };
                        }
                        ctx.steps.push(s);
                    }
                    Err(e) => {
                        self.record(
                            ctx,
                            6,
                            StepOutcome::Failed {
                                reason: format!("le script généré n'a pas pu démarrer : {e}"),
                            },
                            vec![format!("script : {}", path.display())],
                        );
                        ctx.source = Source::None;
                    }
                }
            }
            (Source::None, _) => {
                let reason = if let Some(f) = ctx
                    .steps
                    .iter()
                    .find(|s| matches!(s.outcome, StepOutcome::Failed { .. }))
                {
                    match &f.outcome {
                        StepOutcome::Failed { reason } => format!(
                            "génération impossible après l'échec de l'étape {} : {reason}",
                            f.step
                        ),
                        _ => unreachable!(),
                    }
                } else {
                    format!(
                        "aucun composant enregistré ne produit {} et aucune équation n'a été fournie : impossible de générer un simulateur sans modèle",
                        req.required_outputs.iter().map(|o| format!("`{o}`")).collect::<Vec<_>>().join(", ")
                    )
                };
                self.record(ctx, 6, StepOutcome::Failed { reason }, vec![]);
            }
            (Source::Existing(_), false) => {
                self.skip(ctx, 6, "composant existant non intégrable (voir étape 4)")
            }
        }
    }

    // ------------------------------------------------------------------ 7
    fn step7_test(&self, req: &SimulatorRequest, ctx: &mut Ctx, ctl: &ExecutionControl) {
        let Some(c) = ctx.component.clone() else {
            self.skip(ctx, 7, "aucun composant");
            return;
        };
        let m = c.manifest().clone();
        let mut details = vec![];
        let mut all_checks: Vec<CheckResult> = vec![];
        let cases = &m.validation_protocol.reference_cases;
        if cases.is_empty() {
            details.push("aucun cas de référence déclaré : simulateur NON VALIDÉ (seuls les invariants et la reproductibilité sont contrôlés)".into());
            ctx.extra_assumptions.push("NON VALIDÉ : aucun cas de référence ; résultats à confronter à des références avant tout usage décisionnel".into());
        } else {
            let r = checks::check_reference_cases(c.as_ref(), cases, ctl);
            let failed = r.iter().filter(|x| !x.passed).count();
            details.push(format!("{} cas de référence, {} échec(s)", r.len(), failed));
            all_checks.extend(r);
        }
        let nominal = match Self::nominal_request(req, &m) {
            Ok(r) => r,
            Err(e) => {
                self.record(
                    ctx,
                    7,
                    StepOutcome::Failed {
                        reason: format!("requête nominale : {e}"),
                    },
                    details,
                );
                return;
            }
        };
        let nominal_res = match run_component(c.as_ref(), &nominal, ctl) {
            Ok(r) => r,
            Err(e) => {
                self.record(
                    ctx,
                    7,
                    StepOutcome::Failed {
                        reason: format!("exécution nominale : {e}"),
                    },
                    details,
                );
                ctx.component = None;
                return;
            }
        };
        all_checks.push(checks::check_invariants(
            &m.validation_protocol.invariants,
            &nominal_res,
        ));
        if let Ok(again) = run_component(c.as_ref(), &nominal, ctl) {
            all_checks.push(checks::check_reproducibility(
                &nominal_res,
                &again,
                1e-9,
                1e-12,
            ));
        }
        let origins: Vec<ReferenceOrigin> = cases
            .iter()
            .map(|cse| {
                let s = cse.source.to_lowercase();
                if s.contains("measure")
                    || s.contains("experiment")
                    || s.contains("observ")
                    || s.contains("mesure")
                {
                    ReferenceOrigin::Empirical
                } else {
                    ReferenceOrigin::Analytic
                }
            })
            .collect();
        let record = classify(
            ValidationTarget::Component {
                name: m.name.clone(),
                version: m.version.clone(),
            },
            &nominal_res,
            all_checks,
            &origins,
            None,
            None,
        );
        details.push(format!(
            "verdict du validateur : {}",
            record.verdict.label_fr()
        ));
        let _ = self.ledger.record(
            ProvenanceKind::Validation,
            format!("simulator-factory validation of {}", m.name),
            &record,
            &[],
        );
        let passed = record.all_passed();
        ctx.validation = Some(record);
        if passed {
            self.record(ctx, 7, StepOutcome::Done, details);
        } else {
            self.record(
                ctx,
                7,
                StepOutcome::Failed {
                    reason: "au moins un contrôle de validité a échoué".into(),
                },
                details,
            );
        }
    }

    // ------------------------------------------------------------------ 8
    fn step8_compare(&self, req: &SimulatorRequest, ctx: &mut Ctx, ctl: &ExecutionControl) {
        let Some(c) = ctx.component.clone() else {
            self.skip(ctx, 8, "aucun composant");
            return;
        };
        let m = c.manifest().clone();
        if ctx.python {
            self.skip(ctx, 8, "composant Python externe : le changement de solveur n'est pas exposé par le protocole (RK4 fixe)");
            return;
        }
        if !m.has_capability(&Capability::OdeIntegration) {
            self.skip(
                ctx,
                8,
                "composant algébrique : pas de méthode d'intégration à comparer",
            );
            return;
        }
        let base = match Self::nominal_request(req, &m) {
            Ok(r) => r.fidelity(1.0),
            Err(e) => {
                self.record(
                    ctx,
                    8,
                    StepOutcome::Failed {
                        reason: e.to_string(),
                    },
                    vec![],
                );
                return;
            }
        };
        let numeric = |res: &TaskResult| -> IndexMap<String, f64> {
            res.outputs
                .iter()
                .filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x)))
                .collect()
        };
        let reference = match run_component(
            c.as_ref(),
            &base
                .clone()
                .config(json!({"solver": "rk45", "rtol": 1e-10, "atol": 1e-12})),
            ctl,
        ) {
            Ok(r) => numeric(&r),
            Err(e) => {
                self.record(
                    ctx,
                    8,
                    StepOutcome::Failed {
                        reason: format!("référence RK45 serrée impossible : {e}"),
                    },
                    vec![],
                );
                return;
            }
        };
        let scale = reference.values().fold(0.0f64, |a, v| a.max(v.abs()));
        let (rtol, atol) = Self::tolerances(req.fidelity);
        let fixed_dt = req.horizon / 2000.0;
        let implicit_dt = req.horizon / 400.0;
        let configs: Vec<(Solver, serde_json::Value, f64, f64)> = vec![
            (
                Solver::Euler,
                json!({"solver": "euler", "dt": fixed_dt}),
                fixed_dt,
                0.0,
            ),
            (
                Solver::Rk4,
                json!({"solver": "rk4", "dt": fixed_dt}),
                fixed_dt,
                0.0,
            ),
            (
                Solver::Rk45,
                json!({"solver": "rk45", "rtol": rtol, "atol": atol}),
                1e-2,
                rtol,
            ),
            (
                Solver::BackwardEuler,
                json!({"solver": "backward_euler", "dt": implicit_dt}),
                implicit_dt,
                0.0,
            ),
            (
                Solver::Trapezoidal,
                json!({"solver": "trapezoidal", "dt": implicit_dt}),
                implicit_dt,
                0.0,
            ),
        ];
        let mut rows = vec![];
        for (solver, cfg, dt, rt) in configs {
            if ctl.check().is_err() {
                break;
            }
            let ctl_run = ExecutionControl::with_timeout(std::time::Duration::from_secs(20));
            match run_component(c.as_ref(), &base.clone().config(cfg), &ctl_run) {
                Ok(r) => {
                    let out = numeric(&r);
                    let mut err: f64 = 0.0;
                    for (k, v) in &reference {
                        if let Some(a) = out.get(k) {
                            err = err.max((a - v).abs() / v.abs().max(1e-9 * scale).max(1e-300));
                        }
                    }
                    rows.push(MethodRow {
                        solver,
                        dt,
                        rtol: rt,
                        relative_error: err,
                        rhs_evals: r.metrics.get("rhs_evals").copied().unwrap_or(f64::NAN),
                        wall_time_ms: r.wall_time_ms as f64,
                        ok: err.is_finite(),
                        message: None,
                    });
                }
                Err(e) => rows.push(MethodRow {
                    solver,
                    dt,
                    rtol: rt,
                    relative_error: f64::INFINITY,
                    rhs_evals: f64::NAN,
                    wall_time_ms: f64::NAN,
                    ok: false,
                    message: Some(e.to_string()),
                }),
            }
        }
        let mc = MethodComparison {
            reference: "RK45 rtol=1e-10 atol=1e-12".into(),
            horizon: req.horizon,
            target_relative_error: req.target_relative_error,
            rows,
            selected: None,
            selected_dt: None,
        };
        let details = mc
            .rows
            .iter()
            .map(|r| {
                format!(
                    "{:?}: erreur {:.2e}, {} évaluations, {:.1} ms{}",
                    r.solver,
                    r.relative_error,
                    r.rhs_evals,
                    r.wall_time_ms,
                    r.message
                        .as_ref()
                        .map(|m| format!(" ({m})"))
                        .unwrap_or_default()
                )
            })
            .collect();
        let _ = self.ledger.record(
            ProvenanceKind::Knowledge,
            format!("method comparison for {}", m.name),
            &mc,
            &[],
        );
        ctx.comparison = Some(mc);
        self.record(ctx, 8, StepOutcome::Done, details);
    }

    // ------------------------------------------------------------------ 9
    fn step9_improve(&self, req: &SimulatorRequest, ctx: &mut Ctx) {
        let Some(c) = ctx.component.clone() else {
            self.skip(ctx, 9, "aucun composant");
            return;
        };
        let mut manifest = c.manifest().clone();
        let mut default_config = None;
        let mut details = vec![];
        if let Some(mc) = ctx.comparison.as_mut() {
            let mut ok: Vec<&MethodRow> = mc
                .rows
                .iter()
                .filter(|r| r.ok && r.relative_error <= req.target_relative_error)
                .collect();
            ok.sort_by(|a, b| {
                a.rhs_evals
                    .partial_cmp(&b.rhs_evals)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            match ok.first() {
                Some(best) => {
                    let cfg = match best.solver {
                        Solver::Rk45 => {
                            json!({"solver": "rk45", "rtol": best.rtol, "atol": best.rtol * 1e-3})
                        }
                        s => {
                            json!({"solver": format!("{s:?}").to_lowercase().replace("backwardeuler", "backward_euler"), "dt": best.dt})
                        }
                    };
                    details.push(format!("méthode la moins coûteuse satisfaisant l'erreur cible {:.1e} : {:?} ({} évaluations, erreur {:.2e})", req.target_relative_error, best.solver, best.rhs_evals, best.relative_error));
                    mc.selected = Some(best.solver);
                    mc.selected_dt = Some(best.dt);
                    manifest.uncertainty_model = UncertaintyModel::RelativeErrorBound {
                        rtol: best.relative_error.max(f64::MIN_POSITIVE),
                    };
                    manifest.validation_protocol.convergence_order =
                        Some(best.solver.order() as f64);
                    default_config = Some(cfg);
                }
                None => {
                    details.push(format!("aucune méthode n'atteint l'erreur cible {:.1e} sur l'horizon : solveur de configuration conservé, incertitude déclarée à partir du meilleur résultat", req.target_relative_error));
                    let best_err = mc
                        .rows
                        .iter()
                        .filter(|r| r.ok)
                        .map(|r| r.relative_error)
                        .fold(f64::INFINITY, f64::min);
                    if best_err.is_finite() {
                        manifest.uncertainty_model =
                            UncertaintyModel::RelativeErrorBound { rtol: best_err };
                    }
                }
            }
        } else {
            details.push(
                "pas de comparaison de méthodes : configuration de l'étape 3 conservée".into(),
            );
            if let Some(o) = &ctx.opts {
                if manifest.has_capability(&Capability::OdeIntegration) && !ctx.python {
                    default_config = Some(
                        json!({"solver": format!("{:?}", o.solver).to_lowercase().replace("backwardeuler", "backward_euler"), "dt": o.dt, "rtol": o.rtol, "atol": o.atol}),
                    );
                }
            }
        }
        let _ = self.ledger.append(
            ProvenanceKind::Improvement,
            format!("solver selection for {}", manifest.name),
            json!({"default_config": default_config, "details": details}),
            &[],
        );
        // Re-wrap with the factory manifest (new version → new id) and default config.
        manifest.version = format!(
            "{}+simorph-factory",
            manifest.version.trim_end_matches("+simorph-factory")
        );
        manifest.id =
            ComponentId::deterministic(&format!("{}@{}", manifest.name, manifest.version));
        manifest.execution_interface = if ctx.python {
            c.manifest().execution_interface.clone()
        } else {
            ExecutionInterface::InProcess
        };
        let mut fc = FactoryComponent::new(c, manifest);
        if let Some(cfg) = default_config {
            fc = fc.with_default_config(cfg);
        }
        ctx.component = Some(Arc::new(fc));
        self.record(ctx, 9, StepOutcome::Done, details);
    }

    // ------------------------------------------------------------------ 10
    fn step10_document(&self, req: &SimulatorRequest, ctx: &mut Ctx) -> String {
        let Some(c) = ctx.component.clone() else {
            self.skip(ctx, 10, "aucun composant");
            return String::new();
        };
        let mut m = c.manifest().clone();
        if ctx.generated {
            m.provenance.generated_by = Some("simorph-simulator-factory".into());
            m.assumptions.push(
                "generated component: evidence level of its outputs is at most `simulated`".into(),
            );
        }
        m.assumptions.append(&mut ctx.extra_assumptions);
        m.assumptions.push(format!("numerical configuration chosen on horizon {} s at fidelity {}; re-run the method comparison for other horizons", req.horizon, req.fidelity));
        if ctx.stiff {
            m.assumptions.push(
                "stiff system detected: implicit scheme; explicit schemes are stability-limited"
                    .into(),
            );
        }
        m.numerical_precision = NumericalPrecision::Float64;
        if let Some((p, sha)) = &ctx.script {
            m.provenance.build_hash = Some(sha.clone());
            m.provenance.citation = Some(format!("generated script {p}"));
        }
        if let Some(v) = &ctx.validation {
            m.assumptions.push(format!(
                "validator verdict at generation time: {}",
                v.verdict.label_fr()
            ));
        }
        // Register the documented component: a wrapper around the configured
        // one so the documented manifest is what callers see.
        let final_comp: Arc<dyn Component> = Arc::new(FactoryComponent::new(c.clone(), m.clone()));
        let mut details = vec![];
        match self.components.register_instance(final_comp.clone()) {
            Ok(name) => {
                details.push(format!("composant `{name}` enregistré (id {})", m.id));
                ctx.final_name = Some(name);
            }
            Err(_) => {
                // The base name is taken by the original component: register the
                // configured, documented variant under a distinct name.
                m.name = format!(
                    "{}+simorph-factory",
                    m.name.trim_end_matches("+simorph-factory")
                );
                m.id = ComponentId::deterministic(&format!("{}@{}", m.name, m.version));
                let variant: Arc<dyn Component> =
                    Arc::new(FactoryComponent::new(c.clone(), m.clone()));
                match self.components.register_instance(variant) {
                    Ok(name) => {
                        details.push(format!(
                            "variante configurée enregistrée sous `{name}` (id {})",
                            m.id
                        ));
                        ctx.final_name = Some(name);
                    }
                    Err(e) => details.push(format!("enregistrement impossible : {e}")),
                }
            }
        }
        if ctx.final_name.is_none() {
            ctx.final_name = Some(m.name.clone());
        }
        let _ = self.ledger.record(
            ProvenanceKind::Component,
            format!("documented manifest {}", m.name),
            &m,
            &[],
        );
        let doc = manifest_markdown(&m, ctx.comparison.as_ref());
        details.push(format!(
            "{} hypothèse(s)/limite(s) documentée(s), incertitude {:?}",
            m.assumptions.len(),
            m.uncertainty_model
        ));
        ctx.final_manifest = Some(m);
        self.record(ctx, 10, StepOutcome::Done, details);
        doc
    }
}

fn manifest_markdown(m: &ComponentManifest, mc: Option<&MethodComparison>) -> String {
    let mut s = format!("### {} v{} ({})\n\n", m.name, m.version, m.provider);
    s.push_str(&format!(
        "Capacités : {:?} — Domaines : {}\n\n",
        m.capabilities,
        m.supported_domains.join(", ")
    ));
    s.push_str("| Port | Sens | Unité | Plage |\n|---|---|---|---|\n");
    for p in &m.input_schema {
        s.push_str(&format!(
            "| {} | entrée | {} | {:?} |\n",
            p.name, p.unit, p.range
        ));
    }
    for p in &m.output_schema {
        s.push_str(&format!(
            "| {} | sortie | {} | {:?} |\n",
            p.name, p.unit, p.range
        ));
    }
    s.push_str("\n**Hypothèses et limites**\n");
    for a in &m.assumptions {
        s.push_str(&format!("- {a}\n"));
    }
    if !m.validity_domain.ranges.is_empty() {
        s.push_str("\n**Domaine de validité** : ");
        s.push_str(
            &m.validity_domain
                .ranges
                .iter()
                .map(|(k, (lo, hi))| format!("{k} ∈ [{lo}, {hi}]"))
                .collect::<Vec<_>>()
                .join(", "),
        );
        s.push('\n');
    }
    s.push_str(&format!(
        "\nIncertitude : {:?} — Précision : {:?} — Déterminisme : {:?}\n",
        m.uncertainty_model, m.numerical_precision, m.deterministic_mode
    ));
    s.push_str(&format!(
        "\nCas de référence : {} — Invariants : {}\n",
        m.validation_protocol.reference_cases.len(),
        m.validation_protocol.invariants.len()
    ));
    if let Some(g) = &m.provenance.generated_by {
        s.push_str(&format!("\nGénéré par : {g}\n"));
    }
    if let Some(mc) = mc {
        s.push('\n');
        s.push_str(&mc.markdown());
    }
    s
}

/// Executable model from the request's equations, with reference cases and domains attached.
fn build_model(req: &SimulatorRequest, eq: &EquationSpec) -> Result<ExpressionModel> {
    let mut m = ExpressionModel::new(
        &req.name,
        &eq.equations.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        &eq.variables,
        &eq.parameters,
    )?;
    for c in &eq.reference_cases {
        m = m.with_reference_case(c.clone());
    }
    Ok(m.with_domains(&eq.domains.iter().map(|s| s.as_str()).collect::<Vec<_>>()))
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

fn uuid_like() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    )
}

/// Expose a generated expression model as a co-simulation slave.
pub fn cosim_slave_for(
    model: Arc<ExpressionModel>,
    input_names: &[&str],
    opts: SolverOptions,
) -> Result<simorph_cosimulation::OdeSlave> {
    simorph_cosimulation::OdeSlave::new(model, input_names, opts, false)
}
