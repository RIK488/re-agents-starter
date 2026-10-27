//! NSGA-II multi-objective optimisation and hypervolume indicator.

use crate::problem::*;
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result, SimorphError};
use simorph_uncertainty::seeded;

/// A member of the final Pareto set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParetoPoint {
    pub x: Vec<f64>,
    pub objectives: Vec<f64>,
    pub feasible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParetoResult {
    pub method: Method,
    pub front: Vec<ParetoPoint>,
    pub evaluations: usize,
    pub generations: usize,
}

#[derive(Debug, Clone)]
pub struct Nsga2Options {
    pub population: usize,
    pub generations: usize,
    pub crossover_prob: f64,
    /// SBX distribution index.
    pub eta_c: f64,
    /// Polynomial mutation distribution index.
    pub eta_m: f64,
}

impl Default for Nsga2Options {
    fn default() -> Self {
        Self {
            population: 100,
            generations: 100,
            crossover_prob: 0.9,
            eta_c: 20.0,
            eta_m: 20.0,
        }
    }
}

/// `a` dominates `b` (all <= and at least one <), with constraint
/// domination: feasible dominates infeasible; less violation dominates.
pub fn dominates(a: &Evaluation, b: &Evaluation) -> bool {
    match (a.is_feasible(), b.is_feasible()) {
        (true, false) => return true,
        (false, true) => return false,
        (false, false) => return a.violation() < b.violation(),
        _ => {}
    }
    let mut strictly = false;
    for (x, y) in a.objectives.iter().zip(&b.objectives) {
        if x > y {
            return false;
        }
        if x < y {
            strictly = true;
        }
    }
    strictly
}

/// Fast non-dominated sorting: returns fronts as index lists.
pub fn non_dominated_sort(evals: &[Evaluation]) -> Vec<Vec<usize>> {
    let n = evals.len();
    let mut dominated_by = vec![0usize; n];
    let mut dominates_list: Vec<Vec<usize>> = vec![vec![]; n];
    for i in 0..n {
        for j in 0..n {
            if i != j && dominates(&evals[i], &evals[j]) {
                dominates_list[i].push(j);
                dominated_by[j] += 1;
            }
        }
    }
    let mut fronts = vec![];
    let mut current: Vec<usize> = (0..n).filter(|&i| dominated_by[i] == 0).collect();
    while !current.is_empty() {
        let mut next = vec![];
        for &i in &current {
            for &j in &dominates_list[i] {
                dominated_by[j] -= 1;
                if dominated_by[j] == 0 {
                    next.push(j);
                }
            }
        }
        fronts.push(current);
        current = next;
    }
    fronts
}

/// Crowding distance of each member of a front.
pub fn crowding_distance(evals: &[Evaluation], front: &[usize]) -> Vec<f64> {
    let m = front.len();
    let mut dist = vec![0.0; m];
    if m == 0 {
        return dist;
    }
    let k = evals[front[0]].objectives.len();
    for obj in 0..k {
        let mut order: Vec<usize> = (0..m).collect();
        order.sort_by(|&a, &b| {
            evals[front[a]].objectives[obj].total_cmp(&evals[front[b]].objectives[obj])
        });
        dist[order[0]] = f64::INFINITY;
        dist[order[m - 1]] = f64::INFINITY;
        let range =
            evals[front[order[m - 1]]].objectives[obj] - evals[front[order[0]]].objectives[obj];
        if range <= 0.0 {
            continue;
        }
        for i in 1..m - 1 {
            dist[order[i]] += (evals[front[order[i + 1]]].objectives[obj]
                - evals[front[order[i - 1]]].objectives[obj])
                / range;
        }
    }
    dist
}

fn sbx<R: rand::Rng>(
    a: &[f64],
    b: &[f64],
    bounds: &[(f64, f64)],
    eta: f64,
    rng: &mut R,
) -> (Vec<f64>, Vec<f64>) {
    let mut c1 = a.to_vec();
    let mut c2 = b.to_vec();
    for j in 0..a.len() {
        if rng.gen::<f64>() > 0.5 || (a[j] - b[j]).abs() < 1e-14 {
            continue;
        }
        let u: f64 = rng.gen();
        let beta = if u <= 0.5 {
            (2.0 * u).powf(1.0 / (eta + 1.0))
        } else {
            (1.0 / (2.0 * (1.0 - u))).powf(1.0 / (eta + 1.0))
        };
        c1[j] = 0.5 * ((1.0 + beta) * a[j] + (1.0 - beta) * b[j]);
        c2[j] = 0.5 * ((1.0 - beta) * a[j] + (1.0 + beta) * b[j]);
    }
    clamp_to_bounds(&mut c1, bounds);
    clamp_to_bounds(&mut c2, bounds);
    (c1, c2)
}

fn polynomial_mutation<R: rand::Rng>(x: &mut [f64], bounds: &[(f64, f64)], eta: f64, rng: &mut R) {
    let pm = 1.0 / x.len() as f64;
    for j in 0..x.len() {
        if rng.gen::<f64>() >= pm {
            continue;
        }
        let (lo, hi) = bounds[j];
        if hi <= lo {
            continue;
        }
        let u: f64 = rng.gen();
        let delta = if u < 0.5 {
            (2.0 * u).powf(1.0 / (eta + 1.0)) - 1.0
        } else {
            1.0 - (2.0 * (1.0 - u)).powf(1.0 / (eta + 1.0))
        };
        x[j] = (x[j] + delta * (hi - lo)).clamp(lo, hi);
    }
}

/// NSGA-II (Deb et al. 2002), limited only by `opts.generations`.
pub fn nsga2(
    p: &dyn ObjectiveFunction,
    opts: &Nsga2Options,
    seed: u64,
    ctl: &ExecutionControl,
) -> Result<ParetoResult> {
    nsga2_budgeted(p, opts, seed, usize::MAX, ctl)
}

/// NSGA-II with a hard evaluation budget: a generation is only started when
/// its whole offspring population fits in `max_evaluations`, so the number
/// of objective calls never exceeds the budget (audit: NSGA-II had no
/// evaluation budget at all).  NaN objectives/constraints are treated as
/// `+∞` (worst) so they cannot sit, undominated, in the front.
pub fn nsga2_budgeted(
    p: &dyn ObjectiveFunction,
    opts: &Nsga2Options,
    seed: u64,
    max_evaluations: usize,
    ctl: &ExecutionControl,
) -> Result<ParetoResult> {
    let n = opts.population.max(4);
    if max_evaluations < n {
        return Err(SimorphError::InvalidInput(format!(
            "NSGA-II: evaluation budget {max_evaluations} is smaller than the population {n}"
        )));
    }
    let guard = crate::problem::BudgetGuard::new(p, max_evaluations);
    nsga2_impl(&guard, opts, seed, max_evaluations, ctl)
}

fn nsga2_impl(
    p: &dyn ObjectiveFunction,
    opts: &Nsga2Options,
    seed: u64,
    max_evaluations: usize,
    ctl: &ExecutionControl,
) -> Result<ParetoResult> {
    let d = check_dim(p)?;
    if p.n_objectives() < 2 {
        return Err(SimorphError::InvalidInput(
            "NSGA-II needs at least two objectives".into(),
        ));
    }
    let bounds = p.bounds();
    let c = Counted::new(p);
    let mut rng = seeded(seed);
    let n = opts.population.max(4);
    let mut pop: Vec<Vec<f64>> = (0..n).map(|_| random_point(&bounds, &mut rng)).collect();
    let mut evals: Vec<Evaluation> = pop.iter().map(|x| c.eval(x)).collect::<Result<_>>()?;
    let mut gens = 0;
    for _ in 0..opts.generations {
        ctl.check()?;
        if c.count().saturating_add(n) > max_evaluations {
            break;
        }
        gens += 1;
        // ranks and crowding for tournament
        let fronts = non_dominated_sort(&evals);
        let mut rank = vec![0usize; n];
        let mut crowd = vec![0.0; n];
        for (r, f) in fronts.iter().enumerate() {
            let cd = crowding_distance(&evals, f);
            for (k, &i) in f.iter().enumerate() {
                rank[i] = r;
                crowd[i] = cd[k];
            }
        }
        let tournament = |rng: &mut simorph_uncertainty::Rng| -> usize {
            let a = rng.gen_range(0..n);
            let b = rng.gen_range(0..n);
            if rank[a] < rank[b] || (rank[a] == rank[b] && crowd[a] > crowd[b]) {
                a
            } else {
                b
            }
        };
        let mut offspring = Vec::with_capacity(n);
        while offspring.len() < n {
            let a = tournament(&mut rng);
            let b = tournament(&mut rng);
            let (mut c1, mut c2) = if rng.gen::<f64>() < opts.crossover_prob {
                sbx(&pop[a], &pop[b], &bounds, opts.eta_c, &mut rng)
            } else {
                (pop[a].clone(), pop[b].clone())
            };
            polynomial_mutation(&mut c1, &bounds, opts.eta_m, &mut rng);
            polynomial_mutation(&mut c2, &bounds, opts.eta_m, &mut rng);
            offspring.push(c1);
            if offspring.len() < n {
                offspring.push(c2);
            }
        }
        let off_evals: Vec<Evaluation> =
            offspring.iter().map(|x| c.eval(x)).collect::<Result<_>>()?;
        // environmental selection on combined population
        let mut comb_x = pop.clone();
        comb_x.extend(offspring);
        let mut comb_e = evals.clone();
        comb_e.extend(off_evals);
        let fronts = non_dominated_sort(&comb_e);
        let mut new_idx: Vec<usize> = vec![];
        for f in fronts {
            if new_idx.len() + f.len() <= n {
                new_idx.extend(f);
            } else {
                let cd = crowding_distance(&comb_e, &f);
                let mut order: Vec<usize> = (0..f.len()).collect();
                order.sort_by(|&a, &b| cd[b].total_cmp(&cd[a]));
                for &k in order.iter().take(n - new_idx.len()) {
                    new_idx.push(f[k]);
                }
                break;
            }
        }
        pop = new_idx.iter().map(|&i| comb_x[i].clone()).collect();
        evals = new_idx.iter().map(|&i| comb_e[i].clone()).collect();
        let _ = d;
    }
    let fronts = non_dominated_sort(&evals);
    let front = fronts
        .first()
        .map(|f| {
            f.iter()
                .map(|&i| ParetoPoint {
                    x: pop[i].clone(),
                    objectives: evals[i].objectives.clone(),
                    feasible: evals[i].is_feasible(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(ParetoResult {
        method: Method::Nsga2,
        front,
        evaluations: c.count(),
        generations: gens,
    })
}

/// Hypervolume dominated by `points` (minimisation) w.r.t. `reference`.
/// Exact for 1 and 2 objectives; Monte Carlo with `mc_samples` (0 means
/// 10 000) for more.  Points with a non-finite coordinate are ignored, and a
/// degenerate box (zero width in some objective) has hypervolume 0 (audit:
/// the Monte Carlo branch panicked on an empty sampling range and returned
/// NaN for `mc_samples = 0`).
pub fn hypervolume(points: &[Vec<f64>], reference: &[f64], mc_samples: usize, seed: u64) -> f64 {
    let pts: Vec<&Vec<f64>> = points
        .iter()
        .filter(|p| {
            p.len() >= reference.len()
                && p.iter()
                    .zip(reference)
                    .all(|(a, r)| a.is_finite() && a <= r)
        })
        .collect();
    if pts.is_empty() || reference.is_empty() {
        return 0.0;
    }
    if reference.len() == 1 {
        let m = pts.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        return reference[0] - m;
    }
    if reference.len() == 2 {
        let mut s: Vec<&Vec<f64>> = pts.clone();
        s.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut hv = 0.0;
        let mut prev_f2 = reference[1];
        for p in s {
            if p[1] < prev_f2 {
                hv += (reference[0] - p[0]) * (prev_f2 - p[1]);
                prev_f2 = p[1];
            }
        }
        return hv;
    }
    // Monte Carlo in the box [min_i, ref]
    let k = reference.len();
    let lo: Vec<f64> = (0..k)
        .map(|j| pts.iter().map(|p| p[j]).fold(f64::INFINITY, f64::min))
        .collect();
    let vol: f64 = (0..k).map(|j| reference[j] - lo[j]).product();
    if !(vol > 0.0) {
        return 0.0;
    }
    let mc_samples = if mc_samples == 0 { 10_000 } else { mc_samples };
    let mut rng = seeded(seed);
    let mut hits = 0usize;
    for _ in 0..mc_samples {
        let z: Vec<f64> = (0..k).map(|j| rng.gen_range(lo[j]..reference[j])).collect();
        if pts.iter().any(|p| p.iter().zip(&z).all(|(a, b)| a <= b)) {
            hits += 1;
        }
    }
    vol * hits as f64 / mc_samples as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::*;

    #[test]
    fn zdt1_front_quality() {
        let p = zdt1_problem(10);
        let r = nsga2(
            &p,
            &Nsga2Options {
                population: 100,
                generations: 150,
                ..Default::default()
            },
            7,
            &ExecutionControl::default(),
        )
        .unwrap();
        let pts: Vec<Vec<f64>> = r.front.iter().map(|q| q.objectives.clone()).collect();
        let hv = hypervolume(&pts, &[1.1, 1.1], 0, 0);
        // exact hypervolume of the true front f2 = 1 - sqrt(f1), f1 ∈ [0,1],
        // w.r.t. (1.1, 1.1):  ∫0^1 (1.1 - (1 - √f1)) df1 + 0.1·1.1
        //                   = 0.1 + 2/3 + 0.11 = 0.87667
        // (audit: the constant used before, 0.7767, was 0.11 too low and
        // made the 96 % criterion pass for a front at 85 % of the ideal).
        let ideal = 0.1 + 2.0 / 3.0 + 0.11;
        assert!(hv > 0.96 * ideal, "hv={hv} ideal={ideal}");
        assert!(r.front.len() > 50);
        // every front point is close to the true front
        for q in &r.front {
            let f2_true = 1.0 - q.objectives[0].sqrt();
            assert!(q.objectives[1] - f2_true < 0.05, "{:?}", q.objectives);
        }
    }

    #[test]
    fn hypervolume_2d_exact() {
        let pts = vec![vec![0.0, 1.0], vec![1.0, 0.0], vec![0.5, 0.5]];
        let hv = hypervolume(&pts, &[2.0, 2.0], 0, 0);
        // staircase: (2-0)*(2-1)=2 ; (2-0.5)*(1-0.5)=0.75 ; (2-1)*(0.5-0)=0.5  => 3.25
        assert!((hv - 3.25).abs() < 1e-12);
        let mc = hypervolume(
            &pts.iter()
                .map(|p| vec![p[0], p[1], 0.0])
                .collect::<Vec<_>>(),
            &[2.0, 2.0, 1.0],
            100_000,
            1,
        );
        assert!((mc - 3.25).abs() < 0.1, "mc={mc}");
    }

    #[test]
    fn non_dominated_sort_basic() {
        let e = vec![
            Evaluation {
                objectives: vec![1.0, 1.0],
                constraints: vec![],
                cost: 0.0,
            },
            Evaluation {
                objectives: vec![0.0, 2.0],
                constraints: vec![],
                cost: 0.0,
            },
            Evaluation {
                objectives: vec![2.0, 2.0],
                constraints: vec![],
                cost: 0.0,
            },
        ];
        let f = non_dominated_sort(&e);
        assert_eq!(f[0].len(), 2);
        assert_eq!(f[1], vec![2]);
    }
}
