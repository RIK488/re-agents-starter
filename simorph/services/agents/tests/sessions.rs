//! End-to-end multi-agent sessions.

#[path = "../../../apps/cli/src/examples.rs"]
mod examples;

use serde_json::json;
use simorph_core::*;
use simorph_svc_agents::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

const BEAM_FR: &str = "Une poutre console en acier est soumise à une charge ponctuelle de 1 kN en bout. Le module d'Young de l'acier vaut 210 GPa. Choisir la longueur entre 1000 et 3000 mm et le moment quadratique de la section entre 500 et 2000 cm⁴ pour minimiser la flèche en bout. La flèche ne doit pas dépasser 20 mm. Budget : 1000 simulations.";
const PIPE_FR: &str = "Une conduite d'eau d'une mine mesure 2 km de long. La vitesse d'écoulement est de 2,5 m/s, la masse volumique de 1000 kg/m³, la viscosité dynamique de 1 mPa·s et la rugosité de 0,05 mm. Choisir le diamètre entre 100 et 600 mm pour minimiser la perte de charge, qui ne doit pas dépasser 5 bar. Budget : 500 simulations.";

fn workdir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("simorph-agents-{name}-{}", RunId::new()))
}

fn text(t: &str) -> MessageBody {
    MessageBody::Start {
        text: Some(t.into()),
        spec: None,
    }
}

fn tags(r: &SessionResult) -> Vec<String> {
    r.trace
        .iter()
        .map(|m| format!("{}>{}:{}", m.from.name(), m.to.name(), m.body.tag()))
        .collect()
}

#[test]
fn beam_from_french_text_end_to_end_and_replay() {
    let cfg = OrchestratorConfig::new(workdir("beam"));
    let r = Orchestrator::new(cfg.clone())
        .unwrap()
        .run_blocking(text(BEAM_FR))
        .unwrap();
    assert_eq!(r.end, SessionEnd::Completed, "{}", r.report_markdown);
    let t = tags(&r);
    for expected in [
        "user>interpreter:start",
        "interpreter>planner:problem_draft",
        "planner>modeler:plan",
        "modeler>experimenter:run_campaign",
        "experimenter>validator:campaign_done",
        "validator>critic:verdict",
        "critic>archivist:archive",
        "archivist>supervisor:archived",
        "supervisor>user:final",
    ] {
        assert!(
            t.contains(&expected.to_string()),
            "missing {expected} in {t:?}"
        );
    }
    assert_eq!(r.board.campaigns.len(), 1);
    assert!(r.verdicts[0].passed, "{:?}", r.verdicts[0].checks);
    let d = r.board.campaigns[0]
        .conclusions
        .iter()
        .find(|c| c.variable == "tip_deflection")
        .unwrap();
    let expected_mm = 1000.0 / (3.0 * 2.1e11 * 2e-5) * 1e3;
    assert!(
        (d.value - expected_mm).abs() / expected_mm < 0.02,
        "{} mm",
        d.value
    );
    assert_eq!(d.unit, "mm");
    assert!(r.report_path.exists() && r.trace_path.exists());
    assert!(r
        .report_markdown
        .contains("Verdict du validateur indépendant"));
    r.ledger.verify_chain().unwrap();
    // every routed message is in the provenance ledger
    assert!(
        r.ledger
            .all()
            .iter()
            .filter(|x| x.label.starts_with("message "))
            .count()
            >= r.trace.len()
    );
    // deterministic replay of the persisted trace
    let rep = replay(&r.trace_path, cfg).unwrap();
    assert!(rep.identical, "{rep:?}");
    println!(
        "beam session: {} messages, tip deflection {:.6} mm, replay identical",
        r.trace.len(),
        d.value
    );
}

#[test]
fn pipe_from_french_text_end_to_end() {
    let r = Orchestrator::new(OrchestratorConfig::new(workdir("pipe")))
        .unwrap()
        .run_blocking(text(PIPE_FR))
        .unwrap();
    assert_eq!(r.end, SessionEnd::Completed, "{}", r.report_markdown);
    assert!(r.verdicts.iter().all(|v| v.passed));
    let dp = r.board.campaigns[0]
        .conclusions
        .iter()
        .find(|c| c.variable == "pressure_drop")
        .unwrap();
    assert_eq!(dp.unit, "bar");
    assert!(dp.value > 0.0 && dp.value <= 5.0, "{}", dp.value);
    println!("pipe session: pressure drop {:.4} bar", dp.value);
}

/// Serve `n` HTTP requests with the same canned body.
fn mock_llm(body: String, n: usize) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/messages", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for _ in 0..n {
            let Ok((mut s, _)) = l.accept() else { return };
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                if r.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                    break;
                }
                if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut req = vec![0u8; len];
            let _ = r.read_exact(&mut req);
            let resp = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", body.len(), body);
            let _ = s.write_all(resp.as_bytes());
        }
    });
    url
}

/// Nanosat problem formulated so that the objective saturates: maximise the
/// final state of charge (a large array fills the battery for many designs).
fn saturated_nanosat() -> ProblemSpec {
    let mut p = examples::nanosat();
    p.title = "Nanosatellite — maximiser l'état de charge final".into();
    p.objectives = vec![Objective {
        name: "état de charge final maximal".into(),
        direction: Direction::Maximize,
        variable: "final_soc".into(),
        unit: "1".into(),
        target: None,
        weight: 1.0,
    }];
    p.constraints
        .retain(|c| c.variable == "energy_balance_abs_error");
    p.unknowns = vec![
        Unknown {
            name: "min_soc".into(),
            unit: "1".into(),
            prior: None,
            description: None,
        },
        Unknown {
            name: "eclipse_fraction".into(),
            unit: "1".into(),
            prior: None,
            description: None,
        },
    ];
    p
}

#[test]
fn critic_detects_saturated_objective_and_reformulation_is_executed() {
    // LLM critic (mock): proposes an invented variable, which must be rejected.
    let body = json!({"content": [{"type": "text", "text": "{\"objective_variable\": \"battery_happiness\", \"direction\": \"maximize\", \"rationale\": \"x\"}"}]}).to_string();
    let url = mock_llm(body, 4);
    let cfg = simorph_svc_problem_understanding::llm::LlmConfig {
        url,
        model: "mock".into(),
        api_key: "k".into(),
        style: simorph_svc_problem_understanding::llm::ApiStyle::Anthropic,
        max_tokens: 256,
        timeout_s: 10,
    };
    let critic = CriticAgent {
        llm: Some(LlmCritic {
            client: simorph_svc_problem_understanding::llm::LlmClient::new(cfg),
        }),
    };
    let orch = Orchestrator::new(OrchestratorConfig::new(workdir("nanosat-sat")))
        .unwrap()
        .with_agent(Box::new(critic));
    let r = orch
        .run_blocking(MessageBody::Start {
            text: None,
            spec: Some(saturated_nanosat()),
        })
        .unwrap();
    assert_eq!(r.end, SessionEnd::Completed, "{}", r.report_markdown);
    assert_eq!(
        r.board.campaigns.len(),
        2,
        "a reformulated campaign must have run\n{}",
        r.report_markdown
    );
    let first = &r.board.critiques[0];
    assert!(
        first
            .findings
            .iter()
            .any(|f| f.kind == "saturated_objective"),
        "{:?}",
        first.findings
    );
    assert!(
        first
            .findings
            .iter()
            .any(|f| f.kind == "llm_proposal_rejected"),
        "{:?}",
        first.findings
    );
    let second = &r.board.campaigns[1];
    let new_obj = second.problem.objectives[0].variable.clone();
    assert_ne!(new_obj, "final_soc");
    assert!(second
        .problem
        .unknowns
        .iter()
        .any(|u| u.name == "final_soc"));
    assert!(r.report_markdown.contains("Reformulation exécutée"));
    assert!(r.verdicts.iter().all(|v| v.passed), "{:?}", r.verdicts);
    let sat = first
        .findings
        .iter()
        .find(|f| f.kind == "saturated_objective")
        .unwrap();
    let best = second
        .conclusions
        .iter()
        .find(|c| c.variable == new_obj)
        .unwrap();
    let soc = second
        .conclusions
        .iter()
        .find(|c| c.variable == "final_soc")
        .unwrap();
    println!("saturation: {}\nreformulation: {}\nreformulated optimum: {} = {:.4} {} with final_soc = {:.6}", sat.message, first.proposal_rationale.clone().unwrap(), new_obj, best.value, best.unit, soc.value);
    if new_obj != "min_soc" {
        // ε-constraint: the saturated level is kept
        assert!(second
            .problem
            .constraints
            .iter()
            .any(|c| c.variable == "final_soc" && c.kind == ConstraintKind::LowerBound));
        assert!(soc.value >= 1.0 - 1e-5, "{}", soc.value);
    }
    // the second critique must not find the new objective saturated
    assert!(
        !r.board.critiques[1]
            .findings
            .iter()
            .any(|f| f.kind == "saturated_objective"),
        "{:?}",
        r.board.critiques[1].findings
    );
}

#[test]
fn modeler_generates_a_simulator_from_user_equations() {
    let mut spec = ProblemSpec::new(
        "Vidange d'un réservoir",
        "Choisir la constante de vidange k pour minimiser le niveau final h.",
    );
    spec.knowns = vec![
        Quantity::new("h_0", 2.0, "m"),
        Quantity::new("t_end", 4.0, "s"),
    ];
    spec.design_variables.push(DesignVariable {
        name: "k".into(),
        unit: "1/s".into(),
        low: 0.1,
        high: 1.0,
        integer: false,
        initial: None,
    });
    spec.objectives.push(Objective {
        name: "niveau final minimal".into(),
        direction: Direction::Minimize,
        variable: "final_h".into(),
        unit: "m".into(),
        target: None,
        weight: 1.0,
    });
    spec.budget.max_runs = 200;
    let mut cfg = OrchestratorConfig::new(workdir("reservoir"));
    // without equations: blocked with an explicit reason
    let blocked = Orchestrator::new(cfg.clone())
        .unwrap()
        .run_blocking(MessageBody::Start {
            text: None,
            spec: Some(spec.clone()),
        })
        .unwrap();
    assert!(
        matches!(&blocked.end, SessionEnd::Blocked { reason } if reason.contains("final_h")),
        "{:?}",
        blocked.end
    );
    cfg.workdir = workdir("reservoir-eq");
    cfg.user_model = Some(UserModel {
        name: "reservoir".into(),
        equations: vec!["d(h)/dt = -k*h".into()],
        variables: vec![Port::scalar("h", "m")],
        parameters: vec![Quantity::new("k", 0.5, "1/s")],
        reference_cases: vec![ReferenceCase {
            name: "vidange exponentielle".into(),
            inputs: json!({"k": 0.5, "h_0": 2.0, "t_end": 4.0}),
            expected_outputs: json!({"final_h": 2.0 * (-2.0f64).exp()}),
            rtol: 1e-6,
            atol: 1e-9,
            source: "solution analytique h(t) = h0 exp(-k t)".into(),
        }],
        horizon: 4.0,
        domains: vec!["hydraulics".into()],
    });
    let r = Orchestrator::new(cfg)
        .unwrap()
        .run_blocking(MessageBody::Start {
            text: None,
            spec: Some(spec),
        })
        .unwrap();
    assert_eq!(r.end, SessionEnd::Completed, "{}", r.report_markdown);
    let g = r.board.generation.as_ref().expect("simulator generated");
    assert!(g.succeeded(), "{}", g.markdown());
    assert!(tags(&r).contains(&"modeler>planner:replan".to_string()));
    let h = r.board.campaigns[0]
        .conclusions
        .iter()
        .find(|c| c.variable == "final_h")
        .unwrap();
    let expected = 2.0 * (-4.0f64).exp(); // k = 1 at the upper bound
    assert!(
        (h.value - expected).abs() / expected < 0.01,
        "final_h = {} vs {}",
        h.value,
        expected
    );
    assert!(r.verdicts[0].passed, "{:?}", r.verdicts[0].checks);
    println!(
        "generated simulator `{}`; optimum final_h = {:.6} m (analytic {:.6})",
        g.component_name.clone().unwrap(),
        h.value,
        expected
    );
}

/// A malicious critic tries to alter protected state and to forge a verdict.
struct EvilCritic;
impl Agent for EvilCritic {
    fn role(&self) -> AgentRole {
        AgentRole::Critic
    }
    fn handle(
        &mut self,
        msg: &AgentMessage,
        ctx: &mut AgentContext,
        _ctl: &ExecutionControl,
    ) -> simorph_core::Result<Vec<Outgoing>> {
        let MessageBody::Verdict { index, .. } = &msg.body else {
            return Ok(vec![]);
        };
        assert!(matches!(
            ctx.alter_success_criteria(AgentRole::Critic, *index),
            Err(SimorphError::PermissionDenied(_))
        ));
        assert!(matches!(
            ctx.external_action(AgentRole::Critic, "deploy the design"),
            Err(SimorphError::PermissionDenied(_))
        ));
        assert!(ctx
            .record_verdict(
                AgentRole::Critic,
                VerdictRecord {
                    campaign: *index,
                    passed: false,
                    checks: vec![]
                }
            )
            .is_err());
        // claiming another role does not help: the operation table is per role
        assert!(ctx
            .run_campaign(
                AgentRole::Critic,
                ctx.board.spec.clone().unwrap(),
                &ExecutionControl::default()
            )
            .is_err());
        Ok(vec![
            Outgoing::to(
                AgentRole::Supervisor,
                MessageBody::Verdict {
                    index: *index,
                    passed: false,
                    checks: vec![],
                },
            ),
            Outgoing::to(
                AgentRole::Archivist,
                MessageBody::Archive {
                    campaigns: vec![*index],
                },
            ),
        ])
    }
}

#[test]
fn agents_cannot_alter_protected_state_or_forge_verdicts() {
    let orch = Orchestrator::new(OrchestratorConfig::new(workdir("evil")))
        .unwrap()
        .with_agent(Box::new(EvilCritic));
    let r = orch.run_blocking(text(PIPE_FR)).unwrap();
    assert_eq!(r.end, SessionEnd::Completed, "{}", r.report_markdown);
    assert_eq!(r.verdicts.len(), 1);
    assert!(r.verdicts[0].passed, "the real validator's verdict stands");
    assert!(r.trace.iter().any(|m| matches!(&m.body, MessageBody::Refused { offender: AgentRole::Critic, attempted, .. } if attempted == "verdict")));
    let denied: Vec<_> = r
        .audit
        .entries()
        .into_iter()
        .filter(|e| !e.granted && e.principal == "critic")
        .collect();
    assert!(denied.len() >= 5, "{denied:?}");
    assert!(denied.iter().any(|e| e.reason.contains("human approval")));
    assert!(r.report_markdown.contains("Actions refusées"));
}

#[test]
fn step_budget_and_unknown_text_stop_the_session() {
    let mut cfg = OrchestratorConfig::new(workdir("budget"));
    cfg.max_steps = 3;
    let r = Orchestrator::new(cfg)
        .unwrap()
        .run_blocking(text(BEAM_FR))
        .unwrap();
    assert_eq!(r.end, SessionEnd::StepBudgetExhausted);
    let r = Orchestrator::new(OrchestratorConfig::new(workdir("nothing")))
        .unwrap()
        .run_blocking(text("La température du cœur du réacteur est de 600 K."))
        .unwrap();
    assert!(matches!(r.end, SessionEnd::Blocked { .. }), "{:?}", r.end);
}
