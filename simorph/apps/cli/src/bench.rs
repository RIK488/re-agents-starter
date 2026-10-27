//! Reference benchmarks: measured, reproducible, archived with environment.

use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_simulation::{simulate, DynamicalModel, Params, Solver, SolverOptions};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchRow {
    pub bench: String,
    pub method: String,
    pub metric: String,
    pub value: f64,
    pub reference: f64,
    pub error: f64,
    pub wall_ms: f64,
    pub evaluations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchReport {
    pub environment: simorph_provenance::EnvironmentCapture,
    pub rows: Vec<BenchRow>,
}

/// Harmonic oscillator x'' = -x, x(0)=1, v(0)=0 → x(t)=cos t.
struct Oscillator;
impl DynamicalModel for Oscillator {
    fn name(&self) -> &str {
        "bench.oscillator"
    }
    fn state_dim(&self) -> usize {
        2
    }
    fn state_names(&self) -> Vec<(String, String)> {
        vec![("x".into(), "m".into()), ("v".into(), "m/s".into())]
    }
    fn parameter_names(&self) -> Vec<(String, String)> {
        vec![]
    }
    fn initial_state(&self, _p: &Params) -> Vec<f64> {
        vec![1.0, 0.0]
    }
    fn rhs(&self, _t: f64, x: &[f64], dx: &mut [f64], _p: &Params) -> Result<()> {
        dx[0] = x[1];
        dx[1] = -x[0];
        Ok(())
    }
}

pub fn run(ctl: &ExecutionControl) -> Result<BenchReport> {
    let mut rows = vec![];
    // 1. integrators on the oscillator over 10 periods
    let t_end = 20.0 * std::f64::consts::PI;
    for (name, opts) in [
        ("euler", SolverOptions::new(Solver::Euler).with_dt(1e-3)),
        ("rk4", SolverOptions::new(Solver::Rk4).with_dt(1e-2)),
        (
            "rk45",
            SolverOptions::new(Solver::Rk45).with_tolerances(1e-8, 1e-10),
        ),
        (
            "trapezoidal",
            SolverOptions::new(Solver::Trapezoidal).with_dt(1e-2),
        ),
        (
            "backward_euler",
            SolverOptions::new(Solver::BackwardEuler).with_dt(1e-3),
        ),
    ] {
        ctl.check()?;
        let t0 = Instant::now();
        let tr = simulate(&Oscillator, &Params::new(), None, (0.0, t_end), &opts, ctl)?;
        let x = tr.states.last().map(|s| s[0]).unwrap_or(f64::NAN);
        let energy: f64 = tr
            .states
            .last()
            .map(|s| 0.5 * (s[0] * s[0] + s[1] * s[1]))
            .unwrap_or(f64::NAN);
        rows.push(BenchRow {
            bench: "oscillator".into(),
            method: name.into(),
            metric: "x(20π)".into(),
            value: x,
            reference: 1.0,
            error: (x - 1.0).abs(),
            wall_ms: t0.elapsed().as_secs_f64() * 1e3,
            evaluations: tr.stats.rhs_evals,
        });
        rows.push(BenchRow {
            bench: "oscillator".into(),
            method: name.into(),
            metric: "energy".into(),
            value: energy,
            reference: 0.5,
            error: (energy - 0.5).abs(),
            wall_ms: 0.0,
            evaluations: tr.stats.rhs_evals,
        });
    }
    // 2. Sobol on Ishigami
    {
        let t0 = Instant::now();
        let pi = std::f64::consts::PI;
        let specs: Vec<simorph_uncertainty::InputSpec> = (1..=3)
            .map(|i| {
                simorph_uncertainty::InputSpec::new(
                    &format!("x{i}"),
                    0.0,
                    Uncertainty::Uniform { low: -pi, high: pi },
                )
            })
            .collect();
        let res = simorph_uncertainty::sobol_indices(
            &specs,
            2048,
            1,
            0,
            |x| {
                Ok(vec![
                    x[0].sin() + 7.0 * x[1].sin().powi(2) + 0.1 * x[2].powi(4) * x[0].sin(),
                ])
            },
            ctl,
        )?;
        let refs = [0.3139, 0.4424, 0.0];
        for (i, r) in refs.iter().enumerate() {
            rows.push(BenchRow {
                bench: "ishigami".into(),
                method: "sobol-saltelli".into(),
                metric: format!("S{}", i + 1),
                value: res.indices[i].first_order,
                reference: *r,
                error: (res.indices[i].first_order - r).abs(),
                wall_ms: t0.elapsed().as_secs_f64() * 1e3,
                evaluations: res.evaluations as u64,
            });
        }
    }
    // 3. optimisers on Rosenbrock 2D
    {
        use simorph_optimization::*;
        let p = benchmarks::rosenbrock_problem(2);
        let budget = Budget::new(4000).seed(3);
        let t0 = Instant::now();
        let r = nelder_mead::nelder_mead(&p, Some(&[-1.2, 1.0]), &budget, ctl)?;
        rows.push(BenchRow {
            bench: "rosenbrock-2d".into(),
            method: "nelder_mead".into(),
            metric: "f*".into(),
            value: r.best_f,
            reference: 0.0,
            error: r.best_f.abs(),
            wall_ms: t0.elapsed().as_secs_f64() * 1e3,
            evaluations: r.evaluations as u64,
        });
        let t0 = Instant::now();
        let r = evolutionary::differential_evolution(
            &p,
            &evolutionary::DeOptions::default(),
            &budget,
            ctl,
        )?;
        rows.push(BenchRow {
            bench: "rosenbrock-2d".into(),
            method: "differential_evolution".into(),
            metric: "f*".into(),
            value: r.best_f,
            reference: 0.0,
            error: r.best_f.abs(),
            wall_ms: t0.elapsed().as_secs_f64() * 1e3,
            evaluations: r.evaluations as u64,
        });
        let t0 = Instant::now();
        let r = evolutionary::cma_es(
            &p,
            Some(&[-1.2, 1.0]),
            &evolutionary::CmaOptions::default(),
            &budget,
            ctl,
        )?;
        rows.push(BenchRow {
            bench: "rosenbrock-2d".into(),
            method: "cma_es".into(),
            metric: "f*".into(),
            value: r.best_f,
            reference: 0.0,
            error: r.best_f.abs(),
            wall_ms: t0.elapsed().as_secs_f64() * 1e3,
            evaluations: r.evaluations as u64,
        });
    }
    Ok(BenchReport {
        environment: simorph_provenance::EnvironmentCapture::capture(),
        rows,
    })
}

pub fn to_markdown(r: &BenchReport) -> String {
    let mut s = String::from("| Banc | Méthode | Métrique | Valeur | Référence | Erreur | Temps (ms) | Évaluations |\n|---|---|---|---|---|---|---|---|\n");
    for row in &r.rows {
        s.push_str(&format!(
            "| {} | {} | {} | {:.6e} | {:.6e} | {:.3e} | {:.1} | {} |\n",
            row.bench,
            row.method,
            row.metric,
            row.value,
            row.reference,
            row.error,
            row.wall_ms,
            row.evaluations
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn bench_runs_and_rk45_is_accurate() {
        let r = super::run(&simorph_core::ExecutionControl::default()).unwrap();
        let rk45 = r
            .rows
            .iter()
            .find(|x| x.method == "rk45" && x.metric == "energy")
            .unwrap();
        assert!(rk45.error < 1e-6);
        assert!(r
            .rows
            .iter()
            .any(|x| x.bench == "ishigami" && x.error < 0.06));
    }
}
