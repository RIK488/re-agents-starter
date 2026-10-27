//! Monte Carlo uncertainty propagation.

use crate::sampling::{sample_inputs, Design, InputSpec};
use crate::stats::{summarize, Summary};
use rayon::prelude::*;
use simorph_core::{ExecutionControl, Result, SimorphError};

/// Result of propagating input uncertainty through a model.
#[derive(Debug, Clone)]
pub struct PropagationResult {
    /// Input samples actually used (n × d).
    pub inputs: Vec<Vec<f64>>,
    /// Model outputs per sample (n × m).
    pub outputs: Vec<Vec<f64>>,
    /// Summary per output column.
    pub summaries: Vec<Summary>,
    /// Number of samples whose evaluation failed (they are excluded).
    pub failures: usize,
    pub failure_messages: Vec<String>,
}

/// Evaluate `f` on every sample in parallel.  The sample set is generated
/// first, so the result does not depend on the number of threads.  Failed
/// evaluations, and evaluations returning a non-finite value, are excluded
/// and counted.
pub fn evaluate_samples<F>(
    samples: &[Vec<f64>],
    f: F,
    ctl: &ExecutionControl,
) -> Result<(Vec<Vec<f64>>, Vec<Vec<f64>>, Vec<String>)>
where
    F: Fn(&[f64]) -> Result<Vec<f64>> + Sync,
{
    let results: Vec<Result<Vec<f64>>> = samples
        .par_iter()
        .map(|x| {
            ctl.check()?;
            f(x)
        })
        .collect();
    ctl.check()?;
    let mut kept_in = vec![];
    let mut kept_out = vec![];
    let mut msgs = vec![];
    for (x, r) in samples.iter().zip(results) {
        match r {
            // audit: NaN/∞ outputs used to enter the statistics (NaN mean)
            Ok(y) if y.iter().any(|v| !v.is_finite()) => {
                msgs.push(format!("non-finite model output {y:?} at {x:?}"))
            }
            Ok(y) => {
                kept_in.push(x.clone());
                kept_out.push(y);
            }
            Err(e) => msgs.push(e.to_string()),
        }
    }
    Ok((kept_in, kept_out, msgs))
}

/// Propagate uncertainty of `inputs` through `f` using `n` samples.
pub fn propagate<F>(
    inputs: &[InputSpec],
    design: Design,
    n: usize,
    seed: u64,
    f: F,
    ctl: &ExecutionControl,
) -> Result<PropagationResult>
where
    F: Fn(&[f64]) -> Result<Vec<f64>> + Sync,
{
    let samples = sample_inputs(design, inputs, n, seed)?;
    let (kept_in, outputs, msgs) = evaluate_samples(&samples, f, ctl)?;
    if outputs.is_empty() {
        return Err(SimorphError::Numerical(
            "all Monte Carlo evaluations failed".into(),
        ));
    }
    let m = outputs[0].len();
    if outputs.iter().any(|o| o.len() != m) {
        return Err(SimorphError::Numerical(
            "model returned outputs of varying length".into(),
        ));
    }
    let summaries = (0..m)
        .map(|k| summarize(&outputs.iter().map(|o| o[k]).collect::<Vec<_>>()))
        .collect();
    Ok(PropagationResult {
        inputs: kept_in,
        outputs,
        summaries,
        failures: msgs.len(),
        failure_messages: msgs,
    })
}

/// Bounds obtained by [`propagate_interval`].  **Not rigorous**: they are
/// the minimum and maximum over the sampled points (all box corners when
/// there are at most 12 interval inputs, plus a Latin hypercube); a model
/// with an interior extremum between samples can exceed them.  Use interval
/// arithmetic or a formal method when guaranteed enclosures are required.
#[derive(Debug, Clone)]
pub struct IntervalPropagation {
    /// (lower, upper) per output, over the successful evaluations.
    pub bounds: Vec<(f64, f64)>,
    pub evaluations: usize,
    pub failures: usize,
    /// Always `false` for this sampling-based method.
    pub rigorous: bool,
    pub method: String,
}

/// Propagate interval (set-valued) inputs without inventing a probability
/// law.  Non-interval inputs are held at their nominal value.
pub fn propagate_interval<F>(
    inputs: &[InputSpec],
    n_interior: usize,
    seed: u64,
    f: F,
    ctl: &ExecutionControl,
) -> Result<IntervalPropagation>
where
    F: Fn(&[f64]) -> Result<Vec<f64>> + Sync,
{
    use simorph_core::Uncertainty;
    let boxes: Vec<(f64, f64)> = inputs
        .iter()
        .map(|s| match &s.uncertainty {
            Uncertainty::Interval { low, high } | Uncertainty::Uniform { low, high } => {
                if high < low {
                    Err(SimorphError::InvalidInput(format!(
                        "{}: interval high < low",
                        s.name
                    )))
                } else {
                    Ok((*low, *high))
                }
            }
            _ => Ok((s.value, s.value)),
        })
        .collect::<Result<_>>()?;
    let free: Vec<usize> = (0..boxes.len())
        .filter(|&i| boxes[i].1 > boxes[i].0)
        .collect();
    let mut pts: Vec<Vec<f64>> = vec![inputs.iter().map(|s| s.value).collect()];
    let corners = free.len() <= 12;
    if corners {
        for mask in 0..(1usize << free.len()) {
            let mut x: Vec<f64> = boxes.iter().map(|b| b.0).collect();
            for (k, &i) in free.iter().enumerate() {
                if mask >> k & 1 == 1 {
                    x[i] = boxes[i].1;
                }
            }
            pts.push(x);
        }
    }
    for u in crate::sampling::latin_hypercube(n_interior, boxes.len(), seed) {
        pts.push(
            u.iter()
                .zip(&boxes)
                .map(|(u, (lo, hi))| lo + u * (hi - lo))
                .collect(),
        );
    }
    let (_, outs, msgs) = evaluate_samples(&pts, f, ctl)?;
    if outs.is_empty() {
        return Err(SimorphError::Numerical(
            "all interval evaluations failed".into(),
        ));
    }
    let m = outs[0].len();
    let bounds = (0..m)
        .map(|k| {
            outs.iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), o| {
                    (a.min(o[k]), b.max(o[k]))
                })
        })
        .collect();
    Ok(IntervalPropagation {
        bounds,
        evaluations: pts.len(),
        failures: msgs.len(),
        rigorous: false,
        method: format!(
            "sampling: nominal{} + {n_interior} LHS points (non-rigorous)",
            if corners { " + all box corners" } else { "" }
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::Uncertainty;

    #[test]
    fn linear_model_variance_matches_analytic() {
        // y = 2 x1 + 3 x2, x1 ~ N(1, 0.5), x2 ~ N(2, 1)  => E=8, Var = 4*0.25 + 9*1 = 10
        let inputs = vec![
            InputSpec::normal("x1", 1.0, 0.5),
            InputSpec::normal("x2", 2.0, 1.0),
        ];
        let r = propagate(
            &inputs,
            Design::LatinHypercube,
            20000,
            1,
            |x| Ok(vec![2.0 * x[0] + 3.0 * x[1]]),
            &ExecutionControl::default(),
        )
        .unwrap();
        let s = &r.summaries[0];
        assert!((s.mean - 8.0).abs() < 0.05, "mean {}", s.mean);
        assert!((s.variance - 10.0).abs() < 0.3, "var {}", s.variance);
        assert_eq!(r.failures, 0);
    }

    #[test]
    fn failures_are_counted_not_hidden() {
        let inputs = vec![InputSpec::uniform("x", 0.0, 1.0)];
        let r = propagate(
            &inputs,
            Design::MonteCarlo,
            100,
            2,
            |x| {
                if x[0] > 0.9 {
                    Err(SimorphError::Numerical("boom".into()))
                } else {
                    Ok(vec![x[0]])
                }
            },
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(r.failures > 0 && r.failures < 30);
        assert_eq!(r.outputs.len() + r.failures, 100);
    }

    #[test]
    fn deterministic_across_runs() {
        let inputs = vec![InputSpec::new(
            "x",
            0.0,
            Uncertainty::Uniform {
                low: -1.0,
                high: 1.0,
            },
        )];
        let a = propagate(
            &inputs,
            Design::MonteCarlo,
            500,
            9,
            |x| Ok(vec![x[0].sin()]),
            &ExecutionControl::default(),
        )
        .unwrap();
        let b = propagate(
            &inputs,
            Design::MonteCarlo,
            500,
            9,
            |x| Ok(vec![x[0].sin()]),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(a.outputs, b.outputs);
    }

    #[test]
    fn cancellation_is_honoured() {
        let inputs = vec![InputSpec::uniform("x", 0.0, 1.0)];
        let ctl = ExecutionControl::default();
        ctl.cancel.cancel();
        assert!(propagate(&inputs, Design::MonteCarlo, 10, 1, |x| Ok(vec![x[0]]), &ctl).is_err());
    }
}
