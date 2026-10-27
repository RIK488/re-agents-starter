//! SIMORPH command-line interface.

use clap::{Parser, Subcommand};
use simorph_cli::cluster::serve_worker;
use simorph_cli::*;
use simorph_core::*;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "simorph",
    version,
    about = "SIMORPH — laboratoire numérique universel"
)]
struct Cli {
    /// Workspace directory (provenance ledger, archives, reports).
    #[arg(long, env = "SIMORPH_DATA", default_value = ".simorph")]
    data: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List the available actions (every one is implemented).
    Actions,
    /// Component registry.
    Registry {
        #[command(subcommand)]
        cmd: RegistryCmd,
    },
    /// Units.
    Units {
        #[command(subcommand)]
        cmd: UnitsCmd,
    },
    /// Problem specifications.
    Problem {
        #[command(subcommand)]
        cmd: ProblemCmd,
    },
    /// Analyse a problem (known / unknown / computable / …).
    Analyse {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Hypotheses and experimental programme.
    Plan {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Campaigns.
    Campaign {
        #[command(subcommand)]
        cmd: CampaignCmd,
    },
    /// Execute one component and validate the result.
    Run {
        component: String,
        #[arg(long = "input", short = 'i')]
        inputs: Vec<String>,
        #[arg(long, default_value_t = 1.0)]
        fidelity: f64,
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Archived experiments.
    Experiment {
        #[command(subcommand)]
        cmd: ExperimentCmd,
    },
    /// Optimisation helpers.
    Optimise {
        #[command(subcommand)]
        cmd: OptimiseCmd,
    },
    /// Uncertainty analyses.
    Uncertainty {
        #[command(subcommand)]
        cmd: UncertaintyCmd,
    },
    /// AI laboratory.
    Lab {
        #[command(subcommand)]
        cmd: LabCmd,
    },
    /// Provenance ledger.
    Provenance {
        #[command(subcommand)]
        cmd: ProvenanceCmd,
    },
    /// Export JSON schemas.
    Schemas {
        #[command(subcommand)]
        cmd: SchemasCmd,
    },
    /// Reference benchmarks.
    Bench {
        #[arg(long)]
        json: bool,
    },
    /// Demonstrations (full campaigns on example problems).
    Demo {
        name: String,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Model factory.
    Model {
        #[command(subcommand)]
        cmd: ModelCmd,
    },
    /// Simulator factory (ten-step pipeline).
    Simulator {
        #[command(subcommand)]
        cmd: SimulatorCmd,
    },
    /// Serve the laboratory components to a coordinator (worker process;
    /// the shared secret is read from SIMORPH_WORKER_SECRET).
    Worker {
        #[arg(long)]
        connect: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        slots: Option<usize>,
    },
    /// Law discovery (symbolic regression).
    Discover {
        #[command(subcommand)]
        cmd: DiscoverCmd,
    },
    /// Closed-loop AI improvement.
    Improve {
        #[command(subcommand)]
        cmd: ImproveCmd,
    },
    /// Understand a natural-language problem statement (FR/EN).
    Understand {
        /// Statement text; use `--file` to read it from a file instead.
        text: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        /// Write the resulting problem specification to this JSON file.
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Evaluation subsystem status (harnesses, sealed suites).
    Evaluation {
        #[command(subcommand)]
        cmd: EvaluationCmd,
    },
    /// Data sources (CSV, TSV, JSON-lines).
    Data {
        #[command(subcommand)]
        cmd: DataCmd,
    },
    /// FMU inspection (FMI 2.0 / 3.0).
    Fmu {
        #[command(subcommand)]
        cmd: FmuCmd,
    },
    /// Supervised multi-agent sessions.
    Agents {
        #[command(subcommand)]
        cmd: AgentsCmd,
    },
    /// Formal verification (Z3 certificates).
    Formal {
        #[command(subcommand)]
        cmd: FormalCmd,
    },
    /// Scientific memory and interdisciplinary transfer.
    Knowledge {
        #[command(subcommand)]
        cmd: KnowledgeCmd,
    },
}

#[derive(Subcommand)]
enum ModelCmd {
    /// Assemble the model chain of a problem file.
    Assemble {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Build (and simulate) a model from an equations request JSON file.
    Equations {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Subcommand)]
enum SimulatorCmd {
    Generate {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Subcommand)]
enum DiscoverCmd {
    /// CSV with `name [unit]` headers.
    Csv {
        file: PathBuf,
        #[arg(long)]
        target: String,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Relative held-out error accepted (≈ measurement noise).
        #[arg(long, default_value_t = 0.01)]
        tolerance: f64,
    },
    /// Rediscover Kepler's third law from integrated orbits.
    Kepler {
        #[arg(long, default_value_t = 11)]
        seed: u64,
    },
}
#[derive(Subcommand)]
enum ImproveCmd {
    Run {
        #[arg(long, default_value = "pipe")]
        target: String,
        #[arg(long)]
        recommended: bool,
    },
}
#[derive(Subcommand)]
enum EvaluationCmd {
    Status,
}
#[derive(Subcommand)]
enum DataCmd {
    /// Schema with validated units, SHA-256, first rows; recorded in provenance.
    Inspect {
        file: PathBuf,
        #[arg(long, default_value_t = 5)]
        rows: usize,
    },
}
#[derive(Subcommand)]
enum FmuCmd {
    /// Read modelDescription.xml and derive the component manifest.
    Inspect { file: PathBuf },
}
#[derive(Subcommand)]
enum AgentsCmd {
    /// Solve a natural-language statement with the seven supervised agents.
    Run {
        text: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Replay a recorded session trace and compare every message.
    Replay { trace: PathBuf },
}
fn statement(text: Option<String>, file: Option<PathBuf>) -> Result<String> {
    match (text, file) {
        (Some(t), None) => Ok(t),
        (None, Some(f)) => Ok(std::fs::read_to_string(f)?),
        _ => Err(SimorphError::InvalidInput(
            "give either a statement or --file".into(),
        )),
    }
}
#[derive(Subcommand)]
enum FormalCmd {
    /// Prove or refute a bounded statement given as a JSON request.
    Check {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Subcommand)]
enum KnowledgeCmd {
    Memory,
    Transfers {
        #[arg(long)]
        domain: String,
    },
}

#[derive(Subcommand)]
enum RegistryCmd {
    /// External engines availability.
    Capabilities,
    List {
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        capability: Option<String>,
    },
    Show {
        name: String,
    },
}
#[derive(Subcommand)]
enum UnitsCmd {
    Convert {
        value: f64,
        from: String,
        to: String,
    },
    Check {
        a: String,
        b: String,
    },
}
#[derive(Subcommand)]
enum ProblemCmd {
    Example { name: String },
    Validate { file: PathBuf },
}
#[derive(Subcommand)]
enum CampaignCmd {
    Run {
        file: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
        /// Run on N local worker processes through the distributed coordinator.
        #[arg(long)]
        workers: Option<usize>,
        /// Concurrent tasks per worker.
        #[arg(long, default_value_t = 2)]
        slots: usize,
    },
    Show {
        id: String,
    },
    Events {
        id: String,
    },
    List,
}
#[derive(Subcommand)]
enum ExperimentCmd {
    List,
    Reproduce { id: String },
}
#[derive(Subcommand)]
enum OptimiseCmd {
    Recommend { file: PathBuf },
}
#[derive(Subcommand)]
enum UncertaintyCmd {
    Sobol {
        component: String,
        #[arg(long = "input", short = 'i')]
        inputs: Vec<String>,
        /// name=std_dev of a normally distributed input
        #[arg(long = "vary")]
        vary: Vec<String>,
        #[arg(long)]
        output: String,
        #[arg(long, default_value_t = 1024)]
        n: usize,
    },
}
#[derive(Subcommand)]
enum LabCmd {
    Suites,
    Run {
        #[arg(long)]
        suite: String,
        #[arg(long, default_value_t = 2)]
        iterations: usize,
    },
}
#[derive(Subcommand)]
enum ProvenanceCmd {
    List {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    Explain {
        id: String,
    },
    Verify,
}
#[derive(Subcommand)]
enum SchemasCmd {
    Export { dir: PathBuf },
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();
    let ctl = ExecutionControl::default();
    match cli.cmd {
        Cmd::Actions => {
            for a in ACTIONS {
                println!(
                    "{:<24} {:<14} {}\n{:<24} CLI : {}\n{:<24} HTTP: {}",
                    a.id, a.space, a.label_fr, "", a.cli, "", a.http
                );
            }
        }
        Cmd::Registry { cmd } => match cmd {
            RegistryCmd::List { domain, capability } => {
                for m in components(domain.as_deref(), capability.as_deref())? {
                    println!(
                        "{:<42} {:<10} domaines: {}  capacités: {:?}",
                        m.name,
                        m.version,
                        m.supported_domains.join(","),
                        m.capabilities
                    );
                }
            }
            RegistryCmd::Capabilities => {
                for e in engine_availability()? {
                    println!(
                        "{:<28} {}",
                        e.engine,
                        if e.available {
                            "disponible".to_string()
                        } else {
                            format!("indisponible : {}", e.reason.unwrap_or_default())
                        }
                    );
                }
                for l in simorph_adapter_scientific_python::capability_report() {
                    println!(
                        "  {:<10} {}",
                        l.module,
                        l.version.unwrap_or_else(|| l.error.unwrap_or_default())
                    );
                }
            }
            RegistryCmd::Show { name } => {
                println!("{}", serde_json::to_string_pretty(&component(&name)?)?)
            }
        },
        Cmd::Units { cmd } => match cmd {
            UnitsCmd::Convert { value, from, to } => println!(
                "{} {from} = {} {to}",
                value,
                simorph_units::convert(value, &from, &to)?
            ),
            UnitsCmd::Check { a, b } => match simorph_units::check_same_dimension(&a, &b) {
                Ok(()) => println!("`{a}` et `{b}` ont la même dimension"),
                Err(e) => println!("incohérent : {e}"),
            },
        },
        Cmd::Problem { cmd } => match cmd {
            ProblemCmd::Example { name } => {
                let p = examples::example(&name).ok_or_else(|| {
                    SimorphError::NotFound(format!(
                        "example `{name}` (available: {})",
                        examples::names().join(", ")
                    ))
                })?;
                println!("{}", serde_json::to_string_pretty(&p)?);
            }
            ProblemCmd::Validate { file } => {
                let p = load_problem(&file)?;
                println!("problème « {} » valide : {} connus, {} inconnus, {} variables, {} objectifs, {} contraintes", p.title, p.knowns.len(), p.unknowns.len(), p.design_variables.len(), p.objectives.len(), p.constraints.len());
            }
        },
        Cmd::Analyse { file, json } => {
            let out = analyse(&load_problem(&file)?)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&out.analysis)?)
            } else {
                println!("{}", out.markdown)
            }
        }
        Cmd::Plan { file, json } => {
            let out = plan(&load_problem(&file)?)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&out.programme)?)
            } else {
                println!("{}", out.markdown)
            }
        }
        Cmd::Campaign { cmd } => match cmd {
            CampaignCmd::Run {
                file,
                out,
                workers,
                slots,
            } => {
                let ws = Workspace::open(out.unwrap_or(cli.data))?;
                let c = match workers {
                    Some(n) if n > 0 => {
                        let (c, m) =
                            run_campaign_distributed(&ws, load_problem(&file)?, n, slots, &ctl)?;
                        eprintln!(
                            "cluster : {} tâches, {:.1} tâches/s, reprises {}",
                            m.completed, m.throughput_tasks_per_s, m.retries
                        );
                        for w in &m.workers {
                            eprintln!(
                                "  worker {} : {} tâches, utilisation {:.2}",
                                w.id, w.completed, w.utilisation
                            );
                        }
                        c
                    }
                    _ => run_campaign(&ws, load_problem(&file)?, &ctl)?,
                };
                println!("{}", c.report_markdown());
                println!(
                    "rapport : {}",
                    ws.root
                        .join("campaigns")
                        .join(c.id.to_string())
                        .join("report.md")
                        .display()
                );
            }
            CampaignCmd::Show { id } => {
                let ws = Workspace::open(cli.data)?;
                println!("{}", load_campaign(&ws, &id)?.report_markdown());
            }
            CampaignCmd::Events { id } => {
                let ws = Workspace::open(cli.data)?;
                for e in load_campaign(&ws, &id)?.events {
                    println!(
                        "{:>4} {} {:<22} {}",
                        e.sequence,
                        e.timestamp.format("%H:%M:%S"),
                        e.kind,
                        e.detail
                    );
                }
            }
            CampaignCmd::List => {
                let ws = Workspace::open(cli.data)?;
                for id in list_campaigns(&ws)? {
                    println!("{id}");
                }
            }
        },
        Cmd::Run {
            component,
            inputs,
            fidelity,
            seed,
        } => {
            let ws = Workspace::open(cli.data)?;
            let out = execute_component(
                Some(&ws),
                &component,
                parse_kv(&inputs)?,
                fidelity,
                seed,
                &ctl,
            )?;
            println!("{}", serde_json::to_string_pretty(&out.result)?);
            println!("{}", out.validation.to_markdown());
        }
        Cmd::Experiment { cmd } => match cmd {
            ExperimentCmd::List => {
                let ws = Workspace::open(cli.data)?;
                for id in ws.archive.list()? {
                    println!("{id}");
                }
            }
            ExperimentCmd::Reproduce { id } => {
                let ws = Workspace::open(cli.data)?;
                let r = reproduce_experiment(&ws, &id, &ctl)?;
                println!("{}", serde_json::to_string_pretty(&r)?);
            }
        },
        Cmd::Optimise { cmd } => match cmd {
            OptimiseCmd::Recommend { file } => {
                let r = recommend_methods(&load_problem(&file)?)?;
                println!("traits : {}", serde_json::to_string(&r.features)?);
                for m in r.ranking {
                    println!(
                        "{:<24} score {:.2}\n    {}",
                        format!("{:?}", m.method),
                        m.score,
                        m.justification.join("\n    ")
                    );
                }
            }
        },
        Cmd::Uncertainty { cmd } => match cmd {
            UncertaintyCmd::Sobol {
                component,
                inputs,
                vary,
                output,
                n,
            } => {
                let vary_map: indexmap::IndexMap<String, f64> = parse_kv(&vary)?
                    .into_iter()
                    .map(|(k, v)| (k, v.as_f64().unwrap_or(0.0)))
                    .collect();
                let r = sobol(&component, parse_kv(&inputs)?, vary_map, &output, n, &ctl)?;
                println!(
                    "sortie `{}` : moyenne {:.6e}, variance {:.6e} ({} évaluations)",
                    r.output, r.result.output_mean, r.result.output_variance, r.result.evaluations
                );
                for i in r.result.indices {
                    println!(
                        "{:<20} S1 = {:.3} [{:.3}, {:.3}]   ST = {:.3} [{:.3}, {:.3}]",
                        i.name,
                        i.first_order,
                        i.first_order_ci95.0,
                        i.first_order_ci95.1,
                        i.total_order,
                        i.total_order_ci95.0,
                        i.total_order_ci95.1
                    );
                }
            }
        },
        Cmd::Lab { cmd } => match cmd {
            LabCmd::Suites => {
                for s in lab::suites()? {
                    println!(
                        "{:<12} {} tâches, métriques {:?}, principale `{}`, hash {}",
                        s.name, s.tasks, s.metrics, s.primary_metric, s.hash
                    );
                }
            }
            LabCmd::Run { suite, iterations } => {
                let ws = Workspace::open(cli.data)?;
                let out = lab::run_lab(Some(&ws), &suite, iterations, &ctl)?;
                println!("{}", out.report_markdown);
            }
        },
        Cmd::Provenance { cmd } => match cmd {
            ProvenanceCmd::List { kind, limit } => {
                let ws = Workspace::open(cli.data)?;
                let l = provenance_list(&ws, kind.as_deref(), limit);
                println!("{} enregistrements au total", l.total);
                for r in l.records {
                    println!(
                        "{:>6} {} {:<20} {} {}",
                        r.sequence,
                        r.timestamp.format("%Y-%m-%d %H:%M:%S"),
                        format!("{:?}", r.kind),
                        r.id,
                        r.label
                    );
                }
            }
            ProvenanceCmd::Explain { id } => {
                let ws = Workspace::open(cli.data)?;
                println!("{}", provenance_explain(&ws, &id)?);
            }
            ProvenanceCmd::Verify => {
                let ws = Workspace::open(cli.data)?;
                ws.ledger.verify_chain()?;
                println!(
                    "chaîne de provenance intègre ({} enregistrements)",
                    ws.ledger.len()
                );
            }
        },
        Cmd::Schemas { cmd } => match cmd {
            SchemasCmd::Export { dir } => {
                std::fs::create_dir_all(&dir)?;
                for f in export_schemas(&dir)? {
                    println!("{}", f.display());
                }
            }
        },
        Cmd::Bench { json } => {
            let r = bench::run(&ctl)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?)
            } else {
                println!("{}", bench::to_markdown(&r))
            }
        }
        Cmd::Model { cmd } => match cmd {
            ModelCmd::Assemble { file, json } => {
                let out = assemble_model(&load_problem(&file)?)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&out.assembly)?)
                } else {
                    println!("{}", out.markdown)
                }
            }
            ModelCmd::Equations { file, json } => {
                let req: EquationsRequest = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
                let out = model_from_equations(&req, &ctl)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&out)?)
                } else {
                    println!("{}", out.markdown)
                }
            }
        },
        Cmd::Simulator { cmd } => match cmd {
            SimulatorCmd::Generate { file, json } => {
                let ws = Workspace::open(cli.data)?;
                let req: GenerateRequest = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
                let out = generate_simulator(&ws, &req, &ctl)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&out.report)?)
                } else {
                    println!("{}", out.markdown)
                }
            }
        },
        Cmd::Worker { connect, id, slots } => {
            let exit = serve_worker(&connect, &id, slots)?;
            eprintln!("worker {id} : {exit:?}");
        }
        Cmd::Discover { cmd } => {
            let ws = Workspace::open(cli.data)?;
            let out = match cmd {
                DiscoverCmd::Csv {
                    file,
                    target,
                    seed,
                    tolerance,
                } => discover_csv(
                    Some(&ws),
                    &std::fs::read_to_string(&file)?,
                    &target,
                    seed,
                    tolerance,
                    &ctl,
                )?,
                DiscoverCmd::Kepler { seed } => discover_kepler(Some(&ws), seed, &ctl)?,
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Cmd::Improve { cmd } => match cmd {
            ImproveCmd::Run {
                target,
                recommended,
            } => {
                let ws = Workspace::open(cli.data)?;
                println!(
                    "{}",
                    run_improvement(&ws, &target, recommended, &ctl)?.to_markdown()
                );
            }
        },
        Cmd::Understand {
            text,
            file,
            out,
            json,
        } => {
            let r = understand_text(&statement(text, file)?)?;
            if let Some(p) = out {
                std::fs::write(&p, serde_json::to_string_pretty(&r.spec)?)?;
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?)
            } else {
                println!("{}", r.to_markdown())
            }
        }
        Cmd::Evaluation {
            cmd: EvaluationCmd::Status,
        } => println!("{}", serde_json::to_string_pretty(&evaluation_status()?)?),
        Cmd::Data {
            cmd: DataCmd::Inspect { file, rows },
        } => {
            let ws = Workspace::open(cli.data)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&inspect_data(Some(&ws), &file, rows)?)?
            );
        }
        Cmd::Fmu {
            cmd: FmuCmd::Inspect { file },
        } => println!("{}", serde_json::to_string_pretty(&inspect_fmu(&file)?)?),
        Cmd::Agents { cmd } => match cmd {
            AgentsCmd::Run { text, file, json } => {
                let ws = Workspace::open(cli.data)?;
                let s = run_agent_session(&ws, &statement(text, file)?)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&s)?)
                } else {
                    println!(
                        "{}\n\nfin : {:?} · messages : {} · validé : {:?}\ntrace : {}",
                        s.report_markdown,
                        s.end,
                        s.messages,
                        s.validated,
                        s.trace_path.display()
                    );
                }
            }
            AgentsCmd::Replay { trace } => {
                let ws = Workspace::open(cli.data)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&replay_agent_session(&ws, &trace)?)?
                );
            }
        },
        Cmd::Formal { cmd } => match cmd {
            FormalCmd::Check { file, json } => {
                let ws = Workspace::open(cli.data)?;
                let req: FormalRequest = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
                let out = formal_check(Some(&ws), &req)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&out)?)
                } else {
                    println!(
                        "{}\nniveau accordé : {}",
                        out.summary,
                        out.evidence.label_fr()
                    );
                }
            }
        },
        Cmd::Knowledge { cmd } => match cmd {
            KnowledgeCmd::Memory => {
                let ws = Workspace::open(cli.data)?;
                println!("{}", memory_report(&ws)?);
            }
            KnowledgeCmd::Transfers { domain } => {
                let ws = Workspace::open(cli.data)?;
                let t = transfers(&ws, &domain)?;
                if t.is_empty() {
                    println!("aucun transfert suggéré vers « {domain} » (la mémoire ne contient pas encore de méthode analogue)");
                }
                for x in t {
                    println!(
                        "{} : {} → {} (confiance {:.2}, statut {}) — réfutation : {}",
                        x.method,
                        x.source_domain,
                        x.target_domain,
                        x.confidence,
                        x.status,
                        x.falsification_criterion
                    );
                }
            }
        },
        Cmd::Demo { name, out } => {
            let p = examples::example(&name).ok_or_else(|| {
                SimorphError::NotFound(format!(
                    "demo `{name}` (available: {})",
                    examples::names().join(", ")
                ))
            })?;
            let ws = Workspace::open(out.unwrap_or(cli.data))?;
            let c = run_campaign(&ws, p, &ctl)?;
            println!("{}", c.report_markdown());
            println!(
                "rapport : {}",
                ws.root
                    .join("campaigns")
                    .join(c.id.to_string())
                    .join("report.md")
                    .display()
            );
        }
    }
    Ok(())
}
