//! The closed improvement loop.

use crate::evaluation::*;
use crate::suite::*;
use crate::targets::*;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_learning::{train, Activation, Mlp, OptimizerConfig, TrainConfig, TrainReport};
use simorph_provenance::{Ledger, ProvenanceKind, RecordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    PipeFlow,
    PendulumPeriod,
}

impl TargetKind {
    pub fn build(self) -> TargetSystem {
        match self {
            TargetKind::PipeFlow => pipe_flow_target(),
            TargetKind::PendulumPeriod => pendulum_target(),
        }
    }
}

/// Where a deliberate corruption is injected (negative controls).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Variant {
    /// Normal operation: samples targeted at the worst cells of the error map.
    Targeted,
    /// Same number of samples drawn uniformly over the domain (baseline).
    Uniform,
    /// A fraction of the samples targeted at the worst cells, the rest uniform
    /// (exploitation + exploration).
    Hybrid { targeted_fraction: f64 },
    /// Meta-level self-improvement of the *method*: at each iteration run the
    /// targeted, hybrid (50 %) and uniform designs, train one candidate per
    /// design and keep the one with the lowest error on the probe set (never on
    /// the sealed suite).  Costs three times the simulations of one strategy.
    Adaptive,
    /// Data pipeline bug *after* validation (e.g. kPa stored as Pa: factor 1e-3).
    CorruptedLabels { factor: f64 },
    /// Buggy simulator output *before* validation (validation must catch what it can).
    BuggySimulator { factor: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImprovementConfig {
    pub target: TargetKind,
    pub seed: u64,
    pub suite_size: usize,
    pub probe_size: usize,
    pub initial_samples: usize,
    pub samples_per_iteration: usize,
    pub max_iterations: usize,
    /// Simulator-run budget (suite and probe excluded).
    pub max_simulations: usize,
    pub hidden: Vec<usize>,
    pub epochs: usize,
    pub learning_rate: f64,
    pub batch_size: usize,
    pub bins: usize,
    pub worst_cells: usize,
    pub min_relative_improvement: f64,
    /// A protected metric may degrade by at most `rel × old + abs`.
    pub protected_rel_tolerance: f64,
    pub protected_abs_tolerance: f64,
    pub monotonicity_tolerance: f64,
    pub bootstrap_resamples: usize,
    pub variant: Variant,
}

impl ImprovementConfig {
    pub fn quick(target: TargetKind) -> Self {
        Self {
            target,
            seed: 7,
            suite_size: 250,
            probe_size: 150,
            initial_samples: 24,
            samples_per_iteration: 48,
            max_iterations: 3,
            max_simulations: 400,
            hidden: vec![24, 24],
            epochs: 600,
            learning_rate: 5e-3,
            batch_size: 16,
            bins: if target == TargetKind::PipeFlow { 3 } else { 4 },
            worst_cells: 3,
            min_relative_improvement: 0.05,
            protected_rel_tolerance: 0.15,
            protected_abs_tolerance: 0.005,
            monotonicity_tolerance: 0.01,
            bootstrap_resamples: 2000,
            variant: Variant::Targeted,
        }
    }
    /// Recommended configuration: adaptive strategy selection on the probe set
    /// (measured over 8 seeds to match or beat uniform sampling on the pendulum
    /// target and to be within 4 % of it on pipe flow; see the crate report),
    /// with a budget sized for three strategies per iteration.
    pub fn recommended(target: TargetKind) -> Self {
        Self {
            variant: Variant::Adaptive,
            max_simulations: 24 + 3 * 3 * 48,
            ..Self::quick(target)
        }
    }

    fn train_config(&self) -> TrainConfig {
        TrainConfig {
            epochs: self.epochs,
            batch_size: self.batch_size,
            optimizer: OptimizerConfig::adam(self.learning_rate),
            seed: self.seed,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimitHypothesis {
    pub statement: String,
    pub falsification: String,
    pub cells: Vec<CellError>,
    /// Share of probe squared error held by the selected cells.
    pub error_share: f64,
    /// Share of the input volume covered by the selected cells.
    pub volume_share: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub accepted: bool,
    pub reasons: Vec<String>,
    pub relative_rmse_improvement: f64,
    pub bootstrap_mean: f64,
    pub bootstrap_ci95: (f64, f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IterationRecord {
    pub index: usize,
    pub variant: Variant,
    pub hypothesis: Option<LimitHypothesis>,
    pub n_requested: usize,
    /// Simulator runs actually spent (> n_requested for the adaptive strategy).
    pub n_simulated: usize,
    /// Adaptive strategy: (strategy, probe-set RMSE) of every trained candidate.
    pub strategy_selection: Vec<(String, f64)>,
    pub n_validated: usize,
    pub n_rejected: usize,
    pub rejection_examples: Vec<String>,
    pub experiment_id: ExperimentId,
    pub dataset_hash: String,
    pub dataset_size: usize,
    pub training: TrainSummary,
    pub candidate: SuiteEvaluation,
    pub decision: Decision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainSummary {
    pub epochs: usize,
    pub final_train_loss: f64,
    pub model_hash: String,
    pub wall_time_s: f64,
}

impl TrainSummary {
    fn from(r: &TrainReport, wall: f64) -> Self {
        Self {
            epochs: r.epochs_run,
            final_train_loss: r.final_train_loss,
            model_hash: r.model_hash.clone(),
            wall_time_s: wall,
        }
    }
}

/// Mutable state of the loop: accepted dataset and model history (for rollback).
pub struct ImprovementState {
    pub target: TargetSystem,
    pub config: ImprovementConfig,
    pub suite: SealedEvaluationSuite,
    pub probe: Vec<Sample>,
    pub dataset: Vec<Sample>,
    pub model: Mlp,
    pub evaluation: SuiteEvaluation,
    history: Vec<(Mlp, Vec<Sample>, SuiteEvaluation)>,
    pub ledger: Ledger,
    pub simulations_used: usize,
    pub rejected_samples: usize,
    suite_record: RecordId,
    model_record: RecordId,
}

fn dataset_hash(ds: &[Sample]) -> String {
    hashing::hash_json(&ds.iter().map(|s| (&s.inputs, s.output)).collect::<Vec<_>>())
        .expect("serialisable")
}

impl ImprovementState {
    /// Steps 1–2 setup: sealed suite, probe set, initial sparse dataset, initial model.
    pub fn initialise(
        config: ImprovementConfig,
        ledger: Ledger,
        ctl: &ExecutionControl,
    ) -> Result<Self> {
        let target = config.target.build();
        let suite = SealedEvaluationSuite::build(
            &target,
            config.suite_size,
            config.seed.wrapping_add(1_000_003),
            ctl,
        )?;
        let suite_record = ledger.record(
            ProvenanceKind::Dataset,
            format!(
                "suite d'évaluation scellée {} ({} points)",
                target.name,
                suite.len()
            ),
            &serde_json::json!({"seal": suite.hash(), "n": suite.len()}),
            &[],
        )?;
        let probe_pts = lhs_points(
            &target,
            config.probe_size,
            config.seed.wrapping_add(2_000_029),
            None,
        )?;
        let (probe, _) = run_and_validate(
            &target,
            &probe_pts,
            config.seed.wrapping_add(2_000_029),
            "ensemble de sondage",
            ctl,
        )?;
        let init_pts = lhs_points(&target, config.initial_samples, config.seed, None)?;
        let (init, _) = run_and_validate(&target, &init_pts, config.seed, "jeu initial", ctl)?;
        let mut st = Self {
            model: Mlp::new(&[1, 1], Activation::Identity, Activation::Identity, 0),
            evaluation: evaluate_on_suite(
                &Mlp::new(
                    &[target.inputs.len(), 1],
                    Activation::Identity,
                    Activation::Identity,
                    0,
                ),
                &target,
                &[],
                "",
            ),
            target,
            suite,
            probe: probe.samples,
            dataset: init.samples,
            history: vec![],
            ledger,
            simulations_used: config.initial_samples,
            rejected_samples: init.rejected.len(),
            suite_record,
            model_record: suite_record,
            config,
        };
        let t0 = std::time::Instant::now();
        let (m, _) = st.train_on(&st.dataset.clone(), ctl)?;
        st.model = m;
        st.evaluation = st.evaluate(&st.model)?;
        st.model_record = st.ledger.record(
            ProvenanceKind::Model,
            format!("surrogate initial ({} échantillons, {:.1} s)", st.dataset.len(), t0.elapsed().as_secs_f64()),
            &serde_json::json!({"model_hash": st.model.content_hash(), "dataset_hash": dataset_hash(&st.dataset), "rmse_log10": st.evaluation.rmse_log10, "suite": st.suite.hash()}),
            &[st.suite_record],
        )?;
        Ok(st)
    }

    /// Controlled training: fixed architecture, budget and seed; from scratch.
    pub fn train_on(&self, data: &[Sample], ctl: &ExecutionControl) -> Result<(Mlp, TrainReport)> {
        let mut sizes = vec![self.target.inputs.len()];
        sizes.extend_from_slice(&self.config.hidden);
        sizes.push(1);
        let mut m = Mlp::new(
            &sizes,
            Activation::Tanh,
            Activation::Identity,
            self.config.seed,
        );
        let x: Vec<Vec<f64>> = data
            .iter()
            .map(|s| self.target.features(&s.inputs))
            .collect();
        let y: Vec<Vec<f64>> = data
            .iter()
            .map(|s| {
                self.target
                    .output_transform
                    .forward(s.output)
                    .map(|v| vec![v])
            })
            .collect::<Result<Vec<_>>>()?;
        let r = train(&mut m, &x, &y, &self.config.train_config(), ctl)?;
        Ok((m, r))
    }

    /// Evaluation on the sealed suite (verified before every read).
    pub fn evaluate(&self, model: &Mlp) -> Result<SuiteEvaluation> {
        let pts = self.suite.points()?;
        Ok(evaluate_on_suite(
            model,
            &self.target,
            &pts,
            &self.suite.hash(),
        ))
    }

    /// Step 2–3: limit identification on the probe set and hypothesis.
    pub fn identify_limits(&self) -> LimitHypothesis {
        let map = error_map(
            &self.model,
            &self.target,
            &self.probe,
            &self.dataset,
            self.config.bins,
        );
        let k = self.config.worst_cells.min(map.len());
        let cells: Vec<CellError> = map.iter().take(k).cloned().collect();
        let error_share: f64 = cells.iter().map(|c| c.share).sum();
        let volume_share = k as f64 / map.len() as f64;
        let mean_density = self.dataset.len() as f64 / map.len() as f64;
        let cell_density = cells.iter().map(|c| c.n_train as f64).sum::<f64>() / k.max(1) as f64;
        LimitHypothesis {
            statement: format!(
                "L'erreur du surrogate se concentre dans {k} cellule(s) ({:.0} % de l'erreur quadratique du sondage pour {:.0} % du volume) ; densité d'entraînement {:.1} contre {:.1} en moyenne : des données ciblées dans ces cellules réduiront l'erreur sur la suite scellée.",
                100.0 * error_share,
                100.0 * volume_share,
                cell_density,
                mean_density
            ),
            falsification: "Après ajout de données validées dans ces cellules et ré-entraînement contrôlé, la RMSE sur la suite scellée ne diminue pas de façon significative (IC 95 % bootstrap apparié contenant 0).".into(),
            cells,
            error_share,
            volume_share,
        }
    }

    fn design_for(
        &self,
        variant: Variant,
        hyp: &LimitHypothesis,
        n: usize,
        seed: u64,
    ) -> Result<Vec<Vec<f64>>> {
        match variant {
            Variant::Uniform => lhs_points(&self.target, n, seed, None),
            Variant::Hybrid { targeted_fraction } => {
                let nt = ((n as f64) * targeted_fraction.clamp(0.0, 1.0)).round() as usize;
                let mut pts = self.targeted(hyp, nt, seed)?;
                pts.extend(lhs_points(
                    &self.target,
                    n - nt,
                    seed.wrapping_add(99_991),
                    None,
                )?);
                Ok(pts)
            }
            _ => self.targeted(hyp, n, seed),
        }
    }

    fn targeted(&self, hyp: &LimitHypothesis, n: usize, seed: u64) -> Result<Vec<Vec<f64>>> {
        if n == 0 {
            return Ok(vec![]);
        }
        {
            {
                let total: f64 = hyp.cells.iter().map(|c| c.sse).sum::<f64>().max(1e-300);
                let mut pts = vec![];
                let mut left = n;
                for (i, c) in hyp.cells.iter().enumerate() {
                    let k = if i + 1 == hyp.cells.len() {
                        left
                    } else {
                        ((n as f64 * c.sse / total).round() as usize)
                            .max(1)
                            .min(left)
                    };
                    left -= k;
                    pts.extend(lhs_points(
                        &self.target,
                        k,
                        seed.wrapping_add(i as u64 * 7_919),
                        Some(&c.cell),
                    )?);
                }
                Ok(pts)
            }
        }
    }

    /// One complete iteration (steps 2–11).  Returns the record; the state is
    /// updated only when the candidate is accepted.
    pub fn iterate(&mut self, index: usize, ctl: &ExecutionControl) -> Result<IterationRecord> {
        let hyp = self.identify_limits();
        let hyp_rec = self.ledger.record(
            ProvenanceKind::Hypothesis,
            hyp.statement.clone(),
            &hyp,
            &[self.model_record],
        )?;
        let n = self.config.samples_per_iteration.min(
            self.config
                .max_simulations
                .saturating_sub(self.simulations_used),
        );
        if n == 0 {
            return Err(SimorphError::InvalidInput(
                "simulation budget exhausted".into(),
            ));
        }
        let seed = self.config.seed.wrapping_add(10_000 * (index as u64 + 1));
        let probe_rmse = |m: &Mlp| -> f64 {
            let e: Vec<f64> = self
                .probe
                .iter()
                .map(|p| point_error(m, &self.target, &p.inputs, p.output))
                .collect();
            (e.iter().map(|v| v * v).sum::<f64>() / e.len().max(1) as f64).sqrt()
        };
        let strategies: Vec<Variant> = match self.config.variant {
            Variant::Adaptive => vec![
                Variant::Targeted,
                Variant::Hybrid {
                    targeted_fraction: 0.5,
                },
                Variant::Uniform,
            ],
            v => vec![v],
        };
        let mut best: Option<(
            f64,
            ValidatedBatch,
            simorph_experiment::ExperimentRecord,
            Mlp,
            TrainReport,
            f64,
            Vec<Sample>,
        )> = None;
        let mut selection = vec![];
        let mut n_simulated = 0;
        for (si, strategy) in strategies.iter().enumerate() {
            let sseed = seed.wrapping_add(si as u64 * 1_000_033);
            let pts = self.design_for(*strategy, &hyp, n, sseed)?;
            let (mut batch, record) = match strategy {
                Variant::BuggySimulator { factor } => run_and_validate(
                    &BuggyView {
                        inner: &self.target,
                        factor: *factor,
                    }
                    .as_target(),
                    &pts,
                    sseed,
                    &format!("itération {index} (simulateur défectueux ×{factor})"),
                    ctl,
                )?,
                _ => run_and_validate(
                    &self.target,
                    &pts,
                    sseed,
                    &format!("itération {index} ({strategy:?})"),
                    ctl,
                )?,
            };
            n_simulated += pts.len();
            if let Variant::CorruptedLabels { factor } = strategy {
                for s in &mut batch.samples {
                    s.output *= factor;
                }
            }
            let mut data = self.dataset.clone();
            data.extend(batch.samples.iter().cloned());
            let t0 = std::time::Instant::now();
            let (cand, tr) = self.train_on(&data, ctl)?;
            let wall = t0.elapsed().as_secs_f64();
            let pr = probe_rmse(&cand);
            selection.push((format!("{strategy:?}"), pr));
            if best.as_ref().map(|b| pr < b.0).unwrap_or(true) {
                best = Some((pr, batch, record, cand, tr, wall, data));
            }
        }
        let (_, batch, record, cand, tr, train_wall, candidate_data) =
            best.expect("at least one strategy");
        self.simulations_used += n_simulated;
        self.rejected_samples += batch.rejected.len();
        let exp_rec = self.ledger.record(ProvenanceKind::Experiment, format!("itération {index} : {} points demandés, {} validés, {} rejetés ({} simulations)", n, batch.samples.len(), batch.rejected.len(), n_simulated), &serde_json::json!({"experiment_id": record.id, "outputs_hash": record.outputs_hash, "variant": self.config.variant, "strategy_selection_on_probe": selection}), &[hyp_rec])?;
        self.ledger.record(ProvenanceKind::Validation, format!("validation indépendante des échantillons de l'itération {index}"), &serde_json::json!({"reference_cases_passed": batch.reference_checks_passed, "rejected": batch.rejected.iter().take(5).collect::<Vec<_>>()}), &[exp_rec])?;
        let ds_hash = dataset_hash(&candidate_data);
        let ds_rec = self.ledger.record(
            ProvenanceKind::Dataset,
            format!(
                "jeu de données candidat ({} échantillons)",
                candidate_data.len()
            ),
            &serde_json::json!({"sha256": ds_hash}),
            &[exp_rec],
        )?;
        let training = TrainSummary::from(&tr, train_wall);
        let model_rec = self.ledger.record(
            ProvenanceKind::Model,
            format!("surrogate candidat itération {index}"),
            &training,
            &[ds_rec],
        )?;
        let eval = self.evaluate(&cand)?;
        let val_rec = self.ledger.record(ProvenanceKind::Validation, format!("évaluation sur la suite scellée {}", eval.suite_hash), &serde_json::json!({"rmse_log10": eval.rmse_log10, "protected_rmse": eval.protected_rmse_log10, "monotonicity_violations": eval.monotonicity_violation_rate}), &[model_rec, self.suite_record])?;
        let decision = self.decide(&self.evaluation, &eval, seed);
        let kind = if decision.accepted {
            ProvenanceKind::Improvement
        } else {
            ProvenanceKind::Decision
        };
        let dec_rec = self.ledger.record(
            kind,
            format!(
                "itération {index} : {}",
                if decision.accepted {
                    "candidat accepté"
                } else {
                    "candidat rejeté"
                }
            ),
            &decision,
            &[val_rec],
        )?;
        if decision.accepted {
            self.history.push((
                std::mem::replace(&mut self.model, cand),
                std::mem::replace(&mut self.dataset, candidate_data.clone()),
                self.evaluation.clone(),
            ));
            self.evaluation = eval.clone();
            self.model_record = dec_rec;
        }
        Ok(IterationRecord {
            index,
            variant: self.config.variant,
            hypothesis: Some(hyp),
            n_requested: n,
            n_simulated,
            strategy_selection: selection,
            n_validated: batch.samples.len(),
            n_rejected: batch.rejected.len(),
            rejection_examples: batch
                .rejected
                .iter()
                .take(3)
                .map(|(_, r)| r.clone())
                .collect(),
            experiment_id: batch.experiment_id,
            dataset_hash: ds_hash,
            dataset_size: candidate_data.len(),
            training,
            candidate: eval,
            decision,
        })
    }

    /// Step 10: acceptance rule.
    pub fn decide(&self, old: &SuiteEvaluation, new: &SuiteEvaluation, seed: u64) -> Decision {
        let mut reasons = vec![];
        let mut ok = true;
        if old.suite_hash != new.suite_hash || new.suite_hash != self.suite.hash() {
            ok = false;
            reasons.push("les évaluations n'utilisent pas la même suite scellée".into());
        }
        let rel = (old.rmse_log10 - new.rmse_log10) / old.rmse_log10.max(1e-300);
        let (m, lo, hi) = paired_bootstrap(
            &old.errors,
            &new.errors,
            self.config.bootstrap_resamples,
            seed,
        );
        if lo > 0.0 && rel >= self.config.min_relative_improvement {
            reasons.push(format!("RMSE log10 {:.4} → {:.4} (−{:.1} %), réduction moyenne de l'erreur quadratique {:.2e}, IC 95 % [{:.2e}, {:.2e}] > 0", old.rmse_log10, new.rmse_log10, 100.0 * rel, m, lo, hi));
        } else {
            ok = false;
            reasons.push(format!("amélioration non démontrée : RMSE log10 {:.4} → {:.4} ({:+.1} %), IC 95 % [{:.2e}, {:.2e}]", old.rmse_log10, new.rmse_log10, -100.0 * rel, lo, hi));
        }
        let limit = old.protected_rmse_log10 * (1.0 + self.config.protected_rel_tolerance)
            + self.config.protected_abs_tolerance;
        if new.protected_rmse_log10 > limit {
            ok = false;
            reasons.push(format!(
                "régression de la métrique protégée « {} » : {:.4} → {:.4} (limite {:.4})",
                new.protected_regime, old.protected_rmse_log10, new.protected_rmse_log10, limit
            ));
        } else {
            reasons.push(format!(
                "métrique protégée « {} » : {:.4} → {:.4} (limite {:.4})",
                new.protected_regime, old.protected_rmse_log10, new.protected_rmse_log10, limit
            ));
        }
        if new.monotonicity_violation_rate
            > old.monotonicity_violation_rate + self.config.monotonicity_tolerance
        {
            ok = false;
            reasons.push(format!(
                "monotonie physique dégradée : {:.3} → {:.3}",
                old.monotonicity_violation_rate, new.monotonicity_violation_rate
            ));
        } else {
            reasons.push(format!(
                "monotonie physique : violations {:.3} → {:.3}",
                old.monotonicity_violation_rate, new.monotonicity_violation_rate
            ));
        }
        Decision {
            accepted: ok,
            reasons,
            relative_rmse_improvement: rel,
            bootstrap_mean: m,
            bootstrap_ci95: (lo, hi),
        }
    }

    /// Step 11: revert the last accepted change.
    pub fn rollback(&mut self) -> Result<()> {
        let (m, d, e) = self
            .history
            .pop()
            .ok_or_else(|| SimorphError::InvalidInput("nothing to roll back".into()))?;
        let dropped = self.model.content_hash();
        self.model = m;
        self.dataset = d;
        self.evaluation = e;
        self.model_record = self.ledger.record(ProvenanceKind::Correction, "retour arrière du surrogate", &serde_json::json!({"dropped_model": dropped, "restored_model": self.model.content_hash()}), &[self.model_record])?;
        Ok(())
    }

    pub fn accepted_versions(&self) -> usize {
        self.history.len()
    }
}

/// A view of a target whose simulator output is multiplied by `factor`
/// (negative control: buggy simulator upstream of validation).
struct BuggyView<'a> {
    inner: &'a TargetSystem,
    factor: f64,
}

struct ScaledComponent {
    inner: std::sync::Arc<dyn Component>,
    output: String,
    factor: f64,
}

impl Component for ScaledComponent {
    fn manifest(&self) -> &ComponentManifest {
        self.inner.manifest()
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        let mut r = self.inner.execute(req, ctl)?;
        if let Some(v) = r.outputs.get(&self.output).and_then(|v| v.as_f64()) {
            r.outputs
                .insert(self.output.clone(), serde_json::json!(v * self.factor));
        }
        Ok(r)
    }
}

impl BuggyView<'_> {
    fn as_target(&self) -> TargetSystem {
        TargetSystem {
            name: format!("{} (défectueux)", self.inner.name),
            component: std::sync::Arc::new(ScaledComponent {
                inner: self.inner.component.clone(),
                output: self.inner.output.clone(),
                factor: self.factor,
            }),
            inputs: self.inner.inputs.clone(),
            fixed: self.inner.fixed.clone(),
            output: self.inner.output.clone(),
            output_unit: self.inner.output_unit.clone(),
            output_transform: self.inner.output_transform,
            bounds: self.inner.bounds,
            protected_regime: self.inner.protected_regime.clone(),
            regime_label: self.inner.regime_label,
            monotone_in: self.inner.monotone_in,
        }
    }
}

/// Run the loop until no significant gain, budget exhausted or `max_iterations`.
pub fn run_improvement_campaign(
    config: ImprovementConfig,
) -> Result<crate::report::ImprovementReport> {
    run_improvement_campaign_with(config, Ledger::in_memory(), &ExecutionControl::default())
}

pub fn run_improvement_campaign_with(
    config: ImprovementConfig,
    ledger: Ledger,
    ctl: &ExecutionControl,
) -> Result<crate::report::ImprovementReport> {
    let start = std::time::Instant::now();
    let mut st = ImprovementState::initialise(config.clone(), ledger.clone(), ctl)?;
    let initial = st.evaluation.clone();
    let initial_size = st.dataset.len();
    let mut iterations = vec![];
    let mut stop = "nombre maximal d'itérations atteint".to_string();
    for i in 1..=config.max_iterations {
        if st.simulations_used >= config.max_simulations {
            stop = "budget de simulations épuisé".into();
            break;
        }
        let rec = st.iterate(i, ctl)?;
        let accepted = rec.decision.accepted;
        iterations.push(rec);
        if !accepted {
            stop = "dernier candidat rejeté : pas de gain significatif".into();
            break;
        }
    }
    ledger.verify_chain()?;
    Ok(crate::report::ImprovementReport {
        target: st.target.name.clone(),
        simulator: st.target.component.manifest().name.clone(),
        config,
        suite_hash: st.suite.hash(),
        suite_size: st.suite.len(),
        probe_size: st.probe.len(),
        initial_dataset_size: initial_size,
        initial,
        iterations,
        final_evaluation: st.evaluation.clone(),
        final_dataset_size: st.dataset.len(),
        accepted_versions: st.accepted_versions(),
        simulations_used: st.simulations_used,
        rejected_samples: st.rejected_samples,
        stop_reason: stop,
        ledger_records: ledger.len(),
        wall_time_s: start.elapsed().as_secs_f64(),
    })
}

/// Control (iv): one iteration with targeted vs uniform sampling of the same
/// size, from the same initial state and seed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SamplingComparison {
    pub initial_rmse: f64,
    pub targeted_rmse: f64,
    pub uniform_rmse: f64,
    /// Half targeted, half uniform.
    pub hybrid_rmse: f64,
    /// Adaptive selection on the probe set (3× simulations).
    pub adaptive_rmse: f64,
    pub targeted_worst_regime: (String, f64),
    pub uniform_worst_regime: (String, f64),
    pub samples: usize,
}

pub fn compare_targeted_vs_uniform(
    config: &ImprovementConfig,
    ctl: &ExecutionControl,
) -> Result<SamplingComparison> {
    let mut a = ImprovementState::initialise(
        ImprovementConfig {
            variant: Variant::Targeted,
            ..config.clone()
        },
        Ledger::in_memory(),
        ctl,
    )?;
    let mut b = ImprovementState::initialise(
        ImprovementConfig {
            variant: Variant::Uniform,
            ..config.clone()
        },
        Ledger::in_memory(),
        ctl,
    )?;
    let mut h = ImprovementState::initialise(
        ImprovementConfig {
            variant: Variant::Hybrid {
                targeted_fraction: 0.5,
            },
            ..config.clone()
        },
        Ledger::in_memory(),
        ctl,
    )?;
    let initial = a.evaluation.rmse_log10;
    let ra = a.iterate(1, ctl)?;
    let rb = b.iterate(1, ctl)?;
    let rh = h.iterate(1, ctl)?;
    let mut d = ImprovementState::initialise(
        ImprovementConfig {
            variant: Variant::Adaptive,
            ..config.clone()
        },
        Ledger::in_memory(),
        ctl,
    )?;
    let rd = d.iterate(1, ctl)?;
    let worst = |e: &SuiteEvaluation| {
        e.rmse_by_regime
            .iter()
            .max_by(|x, y| x.1 .1.partial_cmp(&y.1 .1).unwrap())
            .map(|(k, v)| (k.clone(), v.1))
            .unwrap_or_default()
    };
    Ok(SamplingComparison {
        initial_rmse: initial,
        targeted_rmse: ra.candidate.rmse_log10,
        uniform_rmse: rb.candidate.rmse_log10,
        hybrid_rmse: rh.candidate.rmse_log10,
        adaptive_rmse: rd.candidate.rmse_log10,
        targeted_worst_regime: worst(&ra.candidate),
        uniform_worst_regime: worst(&rb.candidate),
        samples: ra.n_requested,
    })
}
