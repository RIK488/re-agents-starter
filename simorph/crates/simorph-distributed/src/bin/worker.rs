//! `simorph-worker`: serve SIMORPH components to a coordinator.
//!
//! Served components: the engineering plugin (beam, pipe flow, NPV, M/M/1,
//! wind turbine, logistic growth) and three diagnostic components used by the
//! cluster tests: `test.square` (y = x²), `test.sleep` (sleeps `ms`, honours
//! cancellation, returns x² and the worker pid), `test.pid`.
//!
//! The shared secret is read from the environment variable named by
//! `--secret-env` (default `SIMORPH_WORKER_SECRET`), never from the command line.

use clap::Parser;
use simorph_core::*;
use simorph_distributed::{run_worker, WorkerConfig, WorkerExit};
use simorph_plugin_sdk::{ComponentBuilder, ComponentSet};
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "simorph-worker")]
struct Args {
    #[arg(long)]
    connect: String,
    #[arg(long)]
    id: String,
    #[arg(long)]
    slots: Option<usize>,
    #[arg(long, default_value = "SIMORPH_WORKER_SECRET")]
    secret_env: String,
    #[arg(long, default_value_t = 300)]
    heartbeat_ms: u64,
}

fn diagnostic_components(set: &ComponentSet) -> Result<()> {
    set.register(|| {
        Ok(Arc::new(
            ComponentBuilder::new("test.square", "1", "simorph-worker")
                .capability(Capability::AlgebraicSolver)
                .input(Port::scalar("x", "1"))
                .output(Port::scalar("y", "1"))
                .build(|req, _| {
                    let x = req.get_f64("x")?;
                    Ok([("y".to_string(), serde_json::json!(x * x))]
                        .into_iter()
                        .collect())
                })?,
        ) as Arc<dyn Component>)
    })?;
    set.register(|| {
        Ok(Arc::new(
            ComponentBuilder::new("test.sleep", "1", "simorph-worker")
                .capability(Capability::Other("diagnostic".into()))
                .input(Port::scalar("x", "1"))
                .input(Port::scalar("ms", "ms"))
                .output(Port::scalar("y", "1"))
                .output(Port::scalar("pid", "1"))
                .build(|req, ctl| {
                    let x = req.get_f64("x")?;
                    let ms = req.get_f64("ms")? as u64;
                    let start = std::time::Instant::now();
                    while start.elapsed() < Duration::from_millis(ms) {
                        ctl.check()?;
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Ok([
                        ("y".to_string(), serde_json::json!(x * x)),
                        ("pid".to_string(), serde_json::json!(std::process::id())),
                    ]
                    .into_iter()
                    .collect())
                })?,
        ) as Arc<dyn Component>)
    })?;
    set.register(|| {
        Ok(Arc::new(
            ComponentBuilder::new("test.pid", "1", "simorph-worker")
                .capability(Capability::Other("diagnostic".into()))
                .output(Port::scalar("pid", "1"))
                .build(|_, _| {
                    Ok([("pid".to_string(), serde_json::json!(std::process::id()))]
                        .into_iter()
                        .collect())
                })?,
        ) as Arc<dyn Component>)
    })?;
    Ok(())
}

fn main() {
    let args = Args::parse();
    let set = ComponentSet::new();
    if let Err(e) = diagnostic_components(&set) {
        eprintln!("simorph-worker: {e}");
        std::process::exit(2);
    }
    for m in simorph_plugin_engineering::manifests() {
        let id = m.id;
        let _ = set.register(move || {
            simorph_plugin_engineering::build(id)
                .map(|b| Arc::from(b) as Arc<dyn Component>)
                .ok_or_else(|| SimorphError::NotFound(id.to_string()))
        });
    }
    let secret = std::env::var(&args.secret_env)
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.into_bytes());
    let cfg = WorkerConfig {
        id: args.id.clone(),
        secret,
        slots: args.slots,
        heartbeat_interval: Duration::from_millis(args.heartbeat_ms),
    };
    match run_worker(&args.connect, cfg, set) {
        Ok(WorkerExit::Rejected(r)) => {
            eprintln!("simorph-worker {}: rejected by coordinator: {r}", args.id);
            std::process::exit(3);
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("simorph-worker {}: {e}", args.id);
            std::process::exit(1);
        }
    }
}
