//! Calibration observations from data sources.

use crate::source::*;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};

/// Same fields as `simorph_svc_campaign_runtime::Observation` (inputs in the
/// problem's units, output value and standard deviation in the component's
/// output unit), defined here so connectors do not depend on the services.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationObservation {
    pub inputs: IndexMap<String, f64>,
    pub output: String,
    pub value: f64,
    pub std: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StdSpec {
    /// Standard deviation read from a column (converted to the output unit).
    Column(String),
    /// Constant standard deviation in the output unit.
    Constant(f64),
    /// Fraction of the observed value.
    Relative(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationSet {
    pub observations: Vec<CalibrationObservation>,
    /// Rows skipped because a required value was missing.
    pub skipped_rows: usize,
    pub source: SourceProvenance,
}

/// Build observations: `inputs` = (column, model input name, target unit);
/// `output` = (column, model output name, target unit).
pub fn calibration_observations(
    source: &mut dyn DataSource,
    inputs: &[(&str, &str, &str)],
    output: (&str, &str, &str),
    std: StdSpec,
) -> Result<CalibrationSet> {
    let rows = source.read_all(1000)?;
    let schema = source.schema().clone();
    let mut cols = vec![];
    for (c, _, u) in inputs {
        cols.push(column_in(&schema, &rows, c, u)?);
    }
    let out = column_in(&schema, &rows, output.0, output.2)?;
    let sd_col = match &std {
        StdSpec::Column(c) => Some(column_in(&schema, &rows, c, output.2)?),
        _ => None,
    };
    let mut observations = vec![];
    let mut skipped = 0;
    'rows: for i in 0..rows.len() {
        let mut ins = IndexMap::new();
        for ((_, name, _), col) in inputs.iter().zip(&cols) {
            match col[i] {
                Some(v) => {
                    ins.insert(name.to_string(), v);
                }
                None => {
                    skipped += 1;
                    continue 'rows;
                }
            }
        }
        let Some(v) = out[i] else {
            skipped += 1;
            continue;
        };
        let sd = match &std {
            StdSpec::Constant(s) => *s,
            StdSpec::Relative(f) => f * v.abs(),
            StdSpec::Column(_) => match sd_col.as_ref().and_then(|c| c[i]) {
                Some(s) => s,
                None => {
                    skipped += 1;
                    continue;
                }
            },
        };
        if sd.is_nan() || sd <= 0.0 {
            return Err(SimorphError::InvalidInput(format!(
                "row {i}: standard deviation must be > 0 (got {sd})"
            )));
        }
        observations.push(CalibrationObservation {
            inputs: ins,
            output: output.1.to_string(),
            value: v,
            std: sd,
        });
    }
    Ok(CalibrationSet {
        observations,
        skipped_rows: skipped,
        source: source.provenance().clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvSource;

    #[test]
    fn observations_in_model_units() {
        let csv = "load [kN],length [mm],deflection [mm],deflection_std [um]\n1,2000,0.4,20\n2,2000,,20\n1.5,1500,0.25,10\n";
        let mut s = CsvSource::from_bytes(csv.as_bytes(), b',', "mem://bench.csv").unwrap();
        let set = calibration_observations(
            &mut s,
            &[("load", "load", "N"), ("length", "length", "m")],
            ("deflection", "tip_deflection", "m"),
            StdSpec::Column("deflection_std".into()),
        )
        .unwrap();
        assert_eq!(set.observations.len(), 2);
        assert_eq!(set.skipped_rows, 1);
        let o = &set.observations[0];
        assert_eq!(o.inputs["load"], 1000.0);
        assert_eq!(o.inputs["length"], 2.0);
        assert!((o.value - 4e-4).abs() < 1e-15);
        assert!((o.std - 2e-5).abs() < 1e-15);
        assert!(calibration_observations(
            &mut s,
            &[("load", "load", "s")],
            ("deflection", "d", "m"),
            StdSpec::Constant(1.0)
        )
        .is_err());
        let rel = calibration_observations(
            &mut s,
            &[("load", "load", "N")],
            ("deflection", "d", "m"),
            StdSpec::Relative(0.1),
        )
        .unwrap();
        assert!((rel.observations[1].std - 2.5e-5).abs() < 1e-15);
    }
}
