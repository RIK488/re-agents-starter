//! The seven laboratory agents.

use crate::context::*;
use crate::message::*;
use simorph_core::*;
use simorph_svc_problem_understanding as pu;
use simorph_svc_scientific_director as dir;

pub trait Agent: Send {
    fn role(&self) -> AgentRole;
    fn handle(
        &mut self,
        msg: &AgentMessage,
        ctx: &mut AgentContext,
        ctl: &ExecutionControl,
    ) -> Result<Vec<Outgoing>>;
}

fn unexpected(role: AgentRole, msg: &AgentMessage) -> Vec<Outgoing> {
    vec![Outgoing::to(
        AgentRole::Supervisor,
        MessageBody::Blocked {
            reason: format!(
                "{} ne traite pas les messages « {} »",
                role.name(),
                msg.body.tag()
            ),
        },
    )]
}

// ------------------------------------------------------------------ Interpreter

/// Natural language → `ProblemSpec` (deterministic pipeline of
/// `simorph-svc-problem-understanding`; an LLM proposer may be attached but
/// its output is re-validated there).
#[derive(Default)]
pub struct InterpreterAgent;

impl Agent for InterpreterAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Interpreter
    }
    fn handle(
        &mut self,
        msg: &AgentMessage,
        ctx: &mut AgentContext,
        _ctl: &ExecutionControl,
    ) -> Result<Vec<Outgoing>> {
        let MessageBody::Start { text, spec } = &msg.body else {
            return Ok(unexpected(self.role(), msg));
        };
        let manifests = ctx.manifests(self.role())?;
        let (spec, mappings, ambiguities) = match (text, spec) {
            (_, Some(s)) => (s.clone(), 0, vec![]),
            (Some(t), None) => {
                let r = pu::understand(t, &manifests);
                ctx.write_artifact(self.role(), "understanding.md", &r.to_markdown())?;
                let amb: Vec<String> = r
                    .ambiguities
                    .iter()
                    .map(|a| format!("{:?}: {}", a.kind, a.message))
                    .collect();
                let blocking = r.has(pu::AmbiguityKind::NoObjectiveOrUnknown);
                ctx.board.text = Some(t.clone());
                let out = (r.spec.clone(), r.mappings.len(), amb);
                ctx.board.understanding = Some(r);
                if blocking {
                    return Ok(vec![Outgoing::to(AgentRole::Supervisor, MessageBody::Blocked { reason: "le texte ne contient ni objectif ni grandeur à déterminer reconnaissable".into() })]);
                }
                out
            }
            (None, None) => {
                return Ok(vec![Outgoing::to(
                    AgentRole::Supervisor,
                    MessageBody::Blocked {
                        reason: "session sans texte ni spécification".into(),
                    },
                )])
            }
        };
        ctx.board.spec = Some(spec.clone());
        Ok(vec![Outgoing::to(
            AgentRole::Planner,
            MessageBody::ProblemDraft {
                spec,
                mappings,
                ambiguities,
            },
        )])
    }
}

// ------------------------------------------------------------------ Planner

#[derive(Default)]
pub struct PlannerAgent;

impl Agent for PlannerAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Planner
    }
    fn handle(
        &mut self,
        msg: &AgentMessage,
        ctx: &mut AgentContext,
        _ctl: &ExecutionControl,
    ) -> Result<Vec<Outgoing>> {
        match &msg.body {
            MessageBody::ProblemDraft { .. } | MessageBody::Replan { .. } => {}
            _ => return Ok(unexpected(self.role(), msg)),
        }
        let spec = ctx.board.spec.clone().ok_or_else(|| {
            SimorphError::InvalidInput("no specification on the blackboard".into())
        })?;
        let catalog = dir::StaticCatalog::new(ctx.manifests(self.role())?);
        let analysis = dir::analyse(&spec, &catalog)?;
        let hyps = dir::formulate(&spec, &analysis);
        let prog = dir::plan(&spec, &analysis, &hyps);
        let md = format!("{}\n{}", analysis.to_markdown(), prog.to_markdown());
        ctx.write_artifact(self.role(), "plan.md", &md)?;
        ctx.board.plan_markdown = Some(md);
        Ok(vec![Outgoing::to(
            AgentRole::Modeler,
            MessageBody::Plan {
                experiments: prog.experiments.len(),
                candidates: analysis
                    .solution_candidates
                    .iter()
                    .map(|c| c.name.clone())
                    .collect(),
                issues: analysis.dimensional_issues.clone(),
            },
        )])
    }
}

// ------------------------------------------------------------------ Modeler

#[derive(Default)]
pub struct ModelerAgent {
    generated_once: bool,
}

impl Agent for ModelerAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Modeler
    }
    fn handle(
        &mut self,
        msg: &AgentMessage,
        ctx: &mut AgentContext,
        ctl: &ExecutionControl,
    ) -> Result<Vec<Outgoing>> {
        let MessageBody::Plan { .. } = &msg.body else {
            return Ok(unexpected(self.role(), msg));
        };
        let spec = ctx
            .board
            .spec
            .clone()
            .ok_or_else(|| SimorphError::InvalidInput("no specification".into()))?;
        let mut reg = simorph_model_registry::Registry::new();
        for m in ctx.manifests(self.role())? {
            reg.register_manifest(m)?;
        }
        let asm = simorph_svc_model_factory::ModelFactory::new(&reg).assemble(&spec)?;
        let md = simorph_svc_model_factory::assembly_markdown(&asm);
        ctx.write_artifact(self.role(), "model.md", &md)?;
        ctx.board.assembly_markdown = Some(md);
        let chain: Vec<String> = asm.chain.steps.iter().map(|s| s.name.clone()).collect();
        if asm.unresolved.is_empty() {
            return Ok(vec![
                Outgoing::to(
                    AgentRole::Supervisor,
                    MessageBody::ModelReady {
                        complete: asm.is_complete(),
                        chain,
                        unresolved: vec![],
                        generated: ctx
                            .board
                            .generation
                            .as_ref()
                            .and_then(|g| g.component_name.clone()),
                    },
                ),
                Outgoing::to(
                    AgentRole::Experimenter,
                    MessageBody::RunCampaign {
                        spec,
                        reason: "programme initial".into(),
                    },
                ),
            ]);
        }
        // Some required outputs have no producer: try the simulator factory with user equations.
        let Some(um) = ctx
            .board
            .user_model
            .clone()
            .filter(|_| !self.generated_once)
        else {
            return Ok(vec![Outgoing::to(
                AgentRole::Supervisor,
                MessageBody::Blocked { reason: format!("aucun composant ne produit {} et aucune équation n'a été fournie : modèle ou données requis", asm.unresolved.join(", ")) },
            )]);
        };
        self.generated_once = true;
        let mut req = simorph_svc_simulator_factory::SimulatorRequest::new(
            &um.name,
            &asm.unresolved
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        );
        req.knowns = spec.knowns.clone();
        for d in &spec.design_variables {
            if !req.knowns.iter().any(|k| k.name == d.name) {
                req.knowns.push(Quantity::new(
                    &d.name,
                    d.initial.unwrap_or(0.5 * (d.low + d.high)),
                    &d.unit,
                ));
            }
        }
        req.domains = um.domains.clone();
        req.horizon = um.horizon;
        req.equations = Some(simorph_svc_simulator_factory::EquationSpec {
            equations: um.equations.clone(),
            variables: um.variables.clone(),
            parameters: um.parameters.clone(),
            reference_cases: um.reference_cases.clone(),
            domains: um.domains.clone(),
        });
        let rep = ctx.generate_simulator(self.role(), &req, ctl)?;
        ctx.write_artifact(self.role(), "simulator.md", &rep.markdown())?;
        let name = rep.component_name.clone();
        let ok = rep.succeeded();
        ctx.board.generation = Some(rep);
        if !ok {
            return Ok(vec![Outgoing::to(AgentRole::Supervisor, MessageBody::Blocked { reason: "la fabrique de simulateurs n'a pas produit de composant validé (voir simulator.md)".into() })]);
        }
        Ok(vec![
            Outgoing::to(
                AgentRole::Supervisor,
                MessageBody::ModelReady {
                    complete: false,
                    chain,
                    unresolved: asm.unresolved.clone(),
                    generated: name.clone(),
                },
            ),
            Outgoing::to(
                AgentRole::Planner,
                MessageBody::Replan {
                    reason: format!("simulateur généré : {}", name.unwrap_or_default()),
                },
            ),
        ])
    }
}

// ------------------------------------------------------------------ Experimenter

#[derive(Default)]
pub struct ExperimenterAgent;

impl Agent for ExperimenterAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Experimenter
    }
    fn handle(
        &mut self,
        msg: &AgentMessage,
        ctx: &mut AgentContext,
        ctl: &ExecutionControl,
    ) -> Result<Vec<Outgoing>> {
        let MessageBody::RunCampaign { spec, reason } = &msg.body else {
            return Ok(unexpected(self.role(), msg));
        };
        let c = ctx.run_campaign(self.role(), spec.clone(), ctl)?;
        let index = ctx.board.campaigns.len();
        ctx.write_artifact(
            self.role(),
            &format!("campaigns/{index}.json"),
            &serde_json::to_string_pretty(&c)?,
        )?;
        ctx.write_artifact(
            self.role(),
            &format!("campaigns/{index}.md"),
            &c.report_markdown(),
        )?;
        let conclusions = c
            .conclusions
            .iter()
            .map(|k| ConclusionView {
                variable: k.variable.clone(),
                value: k.value,
                unit: k.unit.clone(),
                level: k.level,
            })
            .collect();
        let stop = c
            .stop_reason
            .map(|r| format!("{r:?}"))
            .unwrap_or_else(|| "none".into());
        ctx.board.campaigns.push(c);
        ctx.board.campaign_reasons.push(reason.clone());
        Ok(vec![Outgoing::to(
            AgentRole::Validator,
            MessageBody::CampaignDone {
                index,
                stop_reason: stop,
                conclusions,
            },
        )])
    }
}

// ------------------------------------------------------------------ Validator

/// Independent checks of a finished campaign.  Its verdict can only be
/// recorded by this role (`Operation::RecordVerdict`).
#[derive(Default)]
pub struct ValidatorAgent;

pub fn validate_campaign(c: &dir::Campaign, ledger: &simorph_provenance::Ledger) -> Vec<CheckView> {
    let mut checks = vec![];
    let mut push = |name: &str, passed: bool, details: String| {
        checks.push(CheckView {
            name: name.into(),
            passed,
            details,
        })
    };
    match ledger.verify_chain() {
        Ok(()) => push(
            "provenance_chain",
            true,
            format!("{} enregistrements intègres", ledger.len()),
        ),
        Err(e) => push("provenance_chain", false, e.to_string()),
    }
    let mut bad = vec![];
    for e in &c.programme.experiments {
        let h = simorph_core::hashing::hash_json(&e.success_criteria).unwrap_or_default();
        if h != e.criteria_hash {
            bad.push(e.name.clone());
        }
    }
    for o in &c.outcomes {
        if let Some(e) = c
            .programme
            .experiments
            .iter()
            .find(|e| e.id == o.experiment)
        {
            if o.criteria_hash != e.criteria_hash {
                bad.push(format!("{} (évalué)", e.name));
            }
        }
    }
    push(
        "success_criteria_integrity",
        bad.is_empty(),
        if bad.is_empty() {
            format!("{} critères figés intacts", c.programme.experiments.len())
        } else {
            format!("critères modifiés : {}", bad.join(", "))
        },
    );
    let completed = matches!(c.stop_reason, Some(dir::StopReason::ProgrammeCompleted));
    push(
        "campaign_completed",
        completed,
        format!("arrêt : {:?}", c.stop_reason),
    );
    let validations: Vec<serde_json::Value> = ledger
        .by_kind(simorph_provenance::ProvenanceKind::Validation)
        .into_iter()
        .map(|r| r.payload)
        .collect();
    for k in &c.conclusions {
        if k.value.is_nan() {
            continue;
        }
        let name = format!("conclusion:{}", k.variable);
        let Some(rid) = k.supporting_runs.first() else {
            push(&name, false, "aucun run support".into());
            continue;
        };
        let Some(run) = c
            .outcomes
            .iter()
            .flat_map(|o| o.runs.iter())
            .find(|r| &r.run_id == rid)
        else {
            push(
                &name,
                false,
                "run support introuvable dans la campagne".into(),
            );
            continue;
        };
        let mut issues = vec![];
        match run.outputs.get(&k.variable) {
            Some(v) if (v - k.value).abs() <= 1e-12 * v.abs().max(1.0) => {}
            other => issues.push(format!("valeur du run {other:?} ≠ conclusion {}", k.value)),
        }
        if !run.valid {
            issues.push("run support non validé".into());
        }
        if run.evidence != k.level {
            issues.push(format!(
                "niveau {:?} ≠ niveau accordé au run {:?}",
                k.level, run.evidence
            ));
        }
        for v in &k.validations {
            let id = v.to_string();
            match validations
                .iter()
                .find(|p| p["id"].as_str() == Some(id.as_str()))
            {
                None => issues.push(format!(
                    "enregistrement de validation {id} absent du registre"
                )),
                Some(p) => {
                    let all = p["checks"]
                        .as_array()
                        .map(|a| a.iter().all(|x| x["passed"].as_bool() == Some(true)))
                        .unwrap_or(false);
                    if !all {
                        issues.push(format!("validation {id} contient des contrôles en échec"));
                    }
                }
            }
        }
        for cons in c.problem.constraints.iter().filter(|x| x.hard) {
            if let Some(v) = run.outputs.get(&cons.variable) {
                let ok = match cons.kind {
                    ConstraintKind::UpperBound => *v <= cons.bound + cons.tolerance,
                    ConstraintKind::LowerBound => *v >= cons.bound - cons.tolerance,
                    ConstraintKind::Equality => (v - cons.bound).abs() <= cons.tolerance.max(1e-12),
                };
                if !ok {
                    issues.push(format!(
                        "contrainte « {} » violée par le run retenu ({v})",
                        cons.name
                    ));
                }
            }
        }
        push(
            &name,
            issues.is_empty(),
            if issues.is_empty() {
                format!(
                    "{} = {} [{}] tracé, validé, contraintes respectées",
                    k.variable, k.value, k.unit
                )
            } else {
                issues.join("; ")
            },
        );
    }
    checks
}

impl Agent for ValidatorAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Validator
    }
    fn handle(
        &mut self,
        msg: &AgentMessage,
        ctx: &mut AgentContext,
        _ctl: &ExecutionControl,
    ) -> Result<Vec<Outgoing>> {
        let MessageBody::CampaignDone { index, .. } = &msg.body else {
            return Ok(unexpected(self.role(), msg));
        };
        let c = ctx
            .board
            .campaigns
            .get(*index)
            .ok_or_else(|| SimorphError::NotFound(format!("campaign {index}")))?;
        let checks = validate_campaign(c, ctx.ledger());
        // The verdict judges trustworthiness (integrity, traceability,
        // constraints).  Completeness of the programme is reported as a check
        // but does not by itself invalidate the results that were obtained.
        let passed = checks
            .iter()
            .filter(|c| c.name != "campaign_completed")
            .all(|c| c.passed);
        ctx.record_verdict(
            self.role(),
            VerdictRecord {
                campaign: *index,
                passed,
                checks: checks.clone(),
            },
        )?;
        Ok(vec![Outgoing::to(
            AgentRole::Critic,
            MessageBody::Verdict {
                index: *index,
                passed,
                checks,
            },
        )])
    }
}

// ------------------------------------------------------------------ Critic

/// Optional LLM proposer for the Critic (validated before use).
pub struct LlmCritic {
    pub client: pu::llm::LlmClient,
}

#[derive(Default)]
pub struct CriticAgent {
    pub llm: Option<LlmCritic>,
}

/// Values of `var` over the validated, non-reference runs that satisfy all hard constraints.
fn feasible_values(c: &dir::Campaign, var: &str) -> Vec<f64> {
    c.outcomes
        .iter()
        .flat_map(|o| o.runs.iter())
        .filter(|r| r.valid && !r.reference_case)
        .filter(|r| {
            c.problem.constraints.iter().filter(|k| k.hard).all(|k| {
                match r.outputs.get(&k.variable) {
                    Some(v) => match k.kind {
                        ConstraintKind::UpperBound => *v <= k.bound + k.tolerance,
                        ConstraintKind::LowerBound => *v >= k.bound - k.tolerance,
                        ConstraintKind::Equality => (v - k.bound).abs() <= k.tolerance.max(1e-12),
                    },
                    None => true,
                }
            })
        })
        .filter_map(|r| r.outputs.get(var).copied())
        .filter(|v| v.is_finite())
        .collect()
}

/// (best, number of runs within 1e-6 of the best, fraction).
pub fn plateau(values: &[f64], direction: Direction) -> Option<(f64, usize, f64)> {
    if values.is_empty() {
        return None;
    }
    let best = match direction {
        Direction::Maximize => values.iter().cloned().fold(f64::MIN, f64::max),
        _ => values.iter().cloned().fold(f64::MAX, f64::min),
    };
    let tol = 1e-6 * best.abs().max(1.0);
    let n = values.iter().filter(|v| (*v - best).abs() <= tol).count();
    Some((best, n, n as f64 / values.len() as f64))
}

impl CriticAgent {
    fn critique(
        &self,
        c: &dir::Campaign,
        ctx: &AgentContext,
        role: AgentRole,
    ) -> Result<(Vec<Finding>, Option<Proposal>)> {
        let mut findings = vec![];
        let mut proposal = None;
        if !c.contradictions.iter().all(|k| k.resolved) {
            findings.push(Finding {
                kind: "contradiction".into(),
                message: format!(
                    "{} contradiction(s) non résolue(s) : réplication indépendante nécessaire",
                    c.contradictions.iter().filter(|k| !k.resolved).count()
                ),
            });
        }
        for k in &c.conclusions {
            if k.level == EvidenceLevel::Simulated {
                findings.push(Finding {
                    kind: "evidence".into(),
                    message: format!(
                        "`{}` est un résultat simulé non confronté à des observations",
                        k.variable
                    ),
                });
            }
        }
        // constraint activity at the retained design
        if let Some(obj) = c.problem.objectives.first() {
            if let Some(best) = c.conclusions.iter().find(|k| k.variable == obj.variable) {
                if let Some(run) = best.supporting_runs.first().and_then(|rid| {
                    c.outcomes
                        .iter()
                        .flat_map(|o| o.runs.iter())
                        .find(|r| &r.run_id == rid)
                }) {
                    for cons in &c.problem.constraints {
                        if let Some(v) = run.outputs.get(&cons.variable) {
                            let slack = (cons.bound - v).abs() / cons.bound.abs().max(1e-9);
                            if slack < 0.01 {
                                findings.push(Finding { kind: "active_constraint".into(), message: format!("la contrainte « {} » est active à l'optimum ({v} pour une borne {})", cons.name, cons.bound) });
                            }
                        }
                    }
                }
            }
            // saturated / degenerate objective
            let vals = feasible_values(c, &obj.variable);
            if let Some((best, n, frac)) = plateau(&vals, obj.direction) {
                if n >= 3 && frac >= 0.2 {
                    findings.push(Finding {
                        kind: "saturated_objective".into(),
                        message: format!("objectif `{}` saturé : {n}/{} runs admissibles ({:.0} %) atteignent la meilleure valeur {best} — l'optimum n'est pas unique, la formulation est dégénérée", obj.variable, vals.len(), 100.0 * frac),
                    });
                    proposal = self.reformulation(c, ctx, role, obj, &mut findings)?;
                }
            }
        }
        Ok((findings, proposal))
    }

    fn reformulation(
        &self,
        c: &dir::Campaign,
        ctx: &AgentContext,
        role: AgentRole,
        obj: &Objective,
        findings: &mut Vec<Finding>,
    ) -> Result<Option<Proposal>> {
        let producers = ctx.component_by_output(role, &obj.variable)?;
        // the component that actually ran in this campaign
        let Some(m) = producers.iter().find(|m| {
            c.programme
                .experiments
                .iter()
                .any(|e| e.component_name == m.name)
        }) else {
            return Ok(None);
        };
        let obj_port = m
            .output_schema
            .iter()
            .find(|p| p.name == obj.variable)
            .cloned();
        let dim_of = |u: &str| simorph_units::parse_unit(u).ok().map(|x| x.dimension);
        let obj_dim = obj_port.as_ref().and_then(|p| dim_of(&p.unit));
        let usable = |name: &str| -> bool {
            let vals = feasible_values(c, name);
            if vals.len() < 3 {
                return false;
            }
            let (lo, hi) = vals
                .iter()
                .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
            if hi - lo <= 1e-9 * hi.abs().max(1.0) {
                return false;
            }
            plateau(&vals, obj.direction)
                .map(|(_, _, f)| f < 0.2)
                .unwrap_or(false)
        };
        // Strategy A: switch to a declared quantity of interest (problem
        // unknown, same dimension) that discriminates the designs.
        let mut candidates: Vec<String> =
            c.problem.unknowns.iter().map(|u| u.name.clone()).collect();
        candidates.retain(|n| {
            n != &obj.variable
                && m.output_schema
                    .iter()
                    .any(|p| &p.name == n && dim_of(&p.unit) == obj_dim)
        });
        candidates.dedup();
        let mut chosen: Option<(String, Direction, String)> = None;
        if let Some(llm) = &self.llm {
            let prompt = format!(
                "Objective `{}` ({:?}) of component {} is saturated. Candidate outputs: {}. Answer JSON {{\"objective_variable\":..., \"direction\":\"maximize|minimize\", \"rationale\":...}}.",
                obj.variable, obj.direction, m.name, m.output_schema.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ")
            );
            match llm
                .client
                .complete("You are the critic of a scientific laboratory.", &prompt)
                .and_then(|a| pu::llm::extract_json(&a))
            {
                Ok(v) => {
                    let var = v["objective_variable"].as_str().unwrap_or("").to_string();
                    let dirn = if v["direction"].as_str() == Some("minimize") {
                        Direction::Minimize
                    } else {
                        Direction::Maximize
                    };
                    if candidates.contains(&var) && usable(&var) {
                        chosen = Some((
                            var,
                            dirn,
                            v["rationale"]
                                .as_str()
                                .unwrap_or("proposition LLM")
                                .to_string(),
                        ));
                    } else {
                        findings.push(Finding { kind: "llm_proposal_rejected".into(), message: format!("proposition LLM `{var}` rejetée : pas une grandeur d'intérêt déclarée, admissible et non saturée de {}", m.name) });
                    }
                }
                Err(e) => findings.push(Finding {
                    kind: "llm_unavailable".into(),
                    message: e.to_string(),
                }),
            }
        }
        if chosen.is_none() {
            if let Some(var) = candidates.iter().find(|n| usable(n)) {
                chosen = Some((var.clone(), obj.direction, format!("`{var}` (grandeur d'intérêt déclarée, même dimension) varie sur les runs admissibles sans plateau : il discrimine les conceptions que `{}` ne départage pas", obj.variable)));
            }
        }
        let mut spec = c.problem.clone();
        spec.id = ProblemId::new();
        let rationale;
        if let Some((var, dirn, why)) = chosen {
            let unit = m
                .output_schema
                .iter()
                .find(|p| p.name == var)
                .map(|p| p.unit.clone())
                .unwrap_or_else(|| "1".into());
            spec.title = format!("{} — reformulé ({} {:?})", c.problem.title, var, dirn);
            spec.objectives = vec![Objective {
                name: format!("reformulation : {var}"),
                direction: dirn,
                variable: var.clone(),
                unit,
                target: None,
                weight: 1.0,
            }];
            spec.unknowns.retain(|u| u.name != var);
            if !spec.unknowns.iter().any(|u| u.name == obj.variable) {
                spec.unknowns.push(Unknown {
                    name: obj.variable.clone(),
                    unit: obj.unit.clone(),
                    prior: None,
                    description: Some("ancien objectif saturé".into()),
                });
            }
            rationale = format!("stratégie A (changement d'objectif) : {why}");
        } else {
            // Strategy B (ε-constraint): keep the saturated level as a hard
            // constraint and minimise a resource (mass) that still varies on
            // the plateau.  Among mass outputs, take the one with the most
            // distinct values over the plateau (most sensitive to the design).
            let vals = feasible_values(c, &obj.variable);
            let Some((best, _, _)) = plateau(&vals, obj.direction) else {
                return Ok(None);
            };
            let mass_outputs: Vec<&Port> = m
                .output_schema
                .iter()
                .filter(|p| dim_of(&p.unit) == Some(simorph_units::Dimension::MASS))
                .collect();
            let distinct = |name: &str| {
                let mut v = feasible_values(c, name);
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                v.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * a.abs().max(1.0));
                v.len()
            };
            let Some(res) = mass_outputs
                .iter()
                .filter(|p| distinct(&p.name) >= 3)
                .max_by_key(|p| distinct(&p.name))
            else {
                findings.push(Finding { kind: "no_reformulation".into(), message: "aucune grandeur d'intérêt ni ressource (masse) discriminante pour lever la saturation".into() });
                return Ok(None);
            };
            let tol = 1e-6 * best.abs().max(1.0);
            let (kind, bound) = match obj.direction {
                Direction::Maximize => (ConstraintKind::LowerBound, best - tol),
                _ => (ConstraintKind::UpperBound, best + tol),
            };
            spec.title = format!(
                "{} — reformulé (ε-contrainte sur {}, minimiser {})",
                c.problem.title, obj.variable, res.name
            );
            spec.constraints.push(Constraint {
                name: format!("niveau saturé de {} conservé", obj.variable),
                variable: obj.variable.clone(),
                kind,
                bound,
                unit: obj.unit.clone(),
                tolerance: 0.0,
                hard: true,
            });
            spec.objectives = vec![Objective {
                name: format!("reformulation : {} minimale", res.name),
                direction: Direction::Minimize,
                variable: res.name.clone(),
                unit: res.unit.clone(),
                target: None,
                weight: 1.0,
            }];
            if !spec.unknowns.iter().any(|u| u.name == obj.variable) {
                spec.unknowns.push(Unknown {
                    name: obj.variable.clone(),
                    unit: obj.unit.clone(),
                    prior: None,
                    description: Some("ancien objectif saturé".into()),
                });
            }
            rationale = format!(
                "stratégie B (ε-contrainte) : aucune grandeur d'intérêt déclarée ne discrimine les conceptions ; `{}` est maintenu à son niveau saturé ({best}) comme contrainte et la ressource `{}` (masse, {} valeurs distinctes sur le plateau) est minimisée",
                obj.variable,
                res.name,
                distinct(&res.name)
            );
        }
        Ok(Some(Proposal::Reformulate { spec, rationale }))
    }
}

impl Agent for CriticAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Critic
    }
    fn handle(
        &mut self,
        msg: &AgentMessage,
        ctx: &mut AgentContext,
        _ctl: &ExecutionControl,
    ) -> Result<Vec<Outgoing>> {
        let MessageBody::Verdict { index, passed, .. } = &msg.body else {
            return Ok(unexpected(self.role(), msg));
        };
        let c = ctx
            .board
            .campaigns
            .get(*index)
            .cloned()
            .ok_or_else(|| SimorphError::NotFound(format!("campaign {index}")))?;
        let (mut findings, mut proposal) = self.critique(&c, ctx, self.role())?;
        if !passed {
            findings.push(Finding { kind: "validation_failed".into(), message: "le validateur indépendant a rejeté la campagne : aucune reformulation automatique".into() });
            proposal = None;
        }
        if proposal.is_some() && ctx.board.reformulations >= ctx.max_reformulations {
            findings.push(Finding {
                kind: "reformulation_budget".into(),
                message: "budget de reformulations épuisé".into(),
            });
            proposal = None;
        }
        ctx.board.critiques.push(CritiqueRecord {
            campaign: *index,
            findings: findings.clone(),
            proposal_rationale: proposal
                .as_ref()
                .map(|Proposal::Reformulate { rationale, .. }| rationale.clone()),
        });
        let mut out = vec![Outgoing::to(
            AgentRole::Supervisor,
            MessageBody::Critique {
                index: *index,
                findings,
                proposal: proposal.clone(),
            },
        )];
        match proposal {
            Some(Proposal::Reformulate { spec, rationale }) => {
                ctx.board.reformulations += 1;
                out.push(Outgoing::to(
                    AgentRole::Experimenter,
                    MessageBody::RunCampaign {
                        spec,
                        reason: format!("reformulation proposée par le critique : {rationale}"),
                    },
                ));
            }
            None => out.push(Outgoing::to(
                AgentRole::Archivist,
                MessageBody::Archive {
                    campaigns: (0..ctx.board.campaigns.len()).collect(),
                },
            )),
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------ Archivist

#[derive(Default)]
pub struct ArchivistAgent;

impl Agent for ArchivistAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Archivist
    }
    fn handle(
        &mut self,
        msg: &AgentMessage,
        ctx: &mut AgentContext,
        _ctl: &ExecutionControl,
    ) -> Result<Vec<Outgoing>> {
        let MessageBody::Archive { campaigns } = &msg.body else {
            return Ok(unexpected(self.role(), msg));
        };
        let cs: Vec<dir::Campaign> = campaigns
            .iter()
            .filter_map(|i| ctx.board.campaigns.get(*i).cloned())
            .collect();
        let (n, e, path) = ctx.archive(self.role(), &cs)?;
        ctx.board.archive = Some((n, e, path.clone()));
        Ok(vec![Outgoing::to(
            AgentRole::Supervisor,
            MessageBody::Archived {
                nodes_added: n,
                edges_added: e,
                memory_path: path,
            },
        )])
    }
}
