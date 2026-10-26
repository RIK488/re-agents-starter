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
/// evaluations are excluded and counted.
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
