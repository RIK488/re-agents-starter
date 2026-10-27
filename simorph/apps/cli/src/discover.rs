//! Law discovery action (subsystems 4 and 10): dimension-aware symbolic
//! regression on measured or simulated data, validated on held-out rows.

use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_evolution::discovery as disc;
use simorph_evolution::gp::{DimensionMode, LawVerdict};
use simorph_evolution::Dataset;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryOutput {
    pub title: String,
    pub law: Option<String>,
    pub held_out_error: Option<f64>,
    pub dimension_ok: Option<bool>,
    pub claim_level: EvidenceLevel,
    pub statement: String,
    pub generations: usize,
    pub evaluations: usize,
    pub wall_time_s: f64,
}

fn summarise(title: &str, d: &disc::Discovery) -> DiscoveryOutput {
    let (law, err, dim) = match &d.verdict {
        LawVerdict::Law {
            expression,
            held_out_error,
            dimension_ok,
            ..
        } => (
            Some(expression.clone()),
            Some(*held_out_error),
            Some(*dimension_ok),
        ),
        _ => (None, None, None),
    };
    DiscoveryOutput {
        title: title.into(),
        law,
        held_out_error: err,
        dimension_ok: dim,
        claim_level: d.claim.level,
        statement: d.claim.statement.clone(),
        generations: d.gp.generations,
        evaluations: d.gp.evaluations,
        wall_time_s: d.gp.wall_time_s,
    }
}

/// Parse a CSV with `name [unit]` headers; `target` names the column to explain.
pub fn dataset_from_csv(text: &str, target: &str) -> Result<Dataset> {
    let mut lines = text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
    let header: Vec<(String, String)> = lines
        .next()
        .ok_or_else(|| SimorphError::InvalidInput("empty CSV".into()))?
        .split(',')
        .map(|h| {
            let h = h.trim();
            match (h.find('['), h.rfind(']')) {
                (Some(i), Some(j)) if j > i => {
                    (h[..i].trim().to_string(), h[i + 1..j].trim().to_string())
                }
                _ => (h.to_string(), "1".to_string()),
            }
        })
        .collect();
    for (_, u) in &header {
        simorph_units::parse_unit(u)?;
    }
    let ti = header
        .iter()
        .position(|(n, _)| n == target)
        .ok_or_else(|| SimorphError::InvalidInput(format!("target column `{target}` not found")))?;
    let names: Vec<&str> = header
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != ti)
        .map(|(_, (n, _))| n.as_str())
        .collect();
    let units: Vec<&str> = header
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != ti)
        .map(|(_, (_, u))| u.as_str())
        .collect();
    let mut d = Dataset::new(&names, &units, &header[ti].0, &header[ti].1);
    for (k, line) in lines.enumerate() {
        let cells: Vec<f64> = line
            .split(',')
            .map(|c| {
                c.trim().parse::<f64>().map_err(|_| {
                    SimorphError::InvalidInput(format!("row {}: non-numeric `{c}`", k + 2))
                })
            })
            .collect::<Result<_>>()?;
        if cells.len() != header.len() {
            return Err(SimorphError::InvalidInput(format!(
                "row {}: {} cells, header has {}",
                k + 2,
                cells.len(),
                header.len()
            )));
        }
        let y = cells[ti];
        let x: Vec<f64> = cells
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != ti)
            .map(|(_, v)| *v)
            .collect();
        d.push(x, y);
    }
    Ok(d)
}

/// Discover a law in CSV data: every 5th row is held out.
///
/// The law is promoted from hypothesis to `Simulated` evidence only if its
/// relative held-out error is below `tolerance`: set it from the known
/// measurement noise (a law cannot be expected to beat the noise of the data).
pub fn discover_csv(
    ws: Option<&Workspace>,
    text: &str,
    target: &str,
    seed: u64,
    tolerance: f64,
    ctl: &ExecutionControl,
) -> Result<DiscoveryOutput> {
    let all = dataset_from_csv(text, target)?;
    if all.y.len() < 10 {
        return Err(SimorphError::InvalidInput(
            "at least 10 rows are needed (every 5th row is held out)".into(),
        ));
    }
    let names: Vec<&str> = all.names.iter().map(|s| s.as_str()).collect();
    let units: Vec<&str> = all.units.iter().map(|s| s.as_str()).collect();
    let mut train = Dataset::new(&names, &units, &all.target_name, &all.target_unit);
    let mut held = Dataset::new(&names, &units, &all.target_name, &all.target_unit);
    for (i, (x, y)) in all.x.iter().zip(&all.y).enumerate() {
        if i % 5 == 4 {
            held.push(x.clone(), *y)
        } else {
            train.push(x.clone(), *y)
        }
    }
    let cfg = disc::physics_config(names.len(), DimensionMode::Reject, seed);
    let ledger = ws
        .map(|w| w.ledger.clone())
        .unwrap_or_else(simorph_provenance::Ledger::in_memory);
    let d = disc::discover(
        &format!("loi de `{target}`"),
        &train,
        &held,
        &cfg,
        0.5,
        tolerance,
        None,
        &ledger,
        ctl,
    )?;
    Ok(summarise(&format!("loi de `{target}`"), &d))
}

/// Built-in demonstration: rediscover Kepler's third law from orbits
/// integrated by the two-body simulator (periods measured, not computed).
pub fn discover_kepler(
    ws: Option<&Workspace>,
    seed: u64,
    ctl: &ExecutionControl,
) -> Result<DiscoveryOutput> {
    let train = disc::kepler_dataset(&["Earth", "Moon", "Mars", "Jupiter"], 8, 1e-4, seed, ctl)?;
    let held = disc::kepler_dataset(&["Venus", "Sun"], 8, 1e-4, seed + 1, ctl)?;
    let cfg = disc::physics_config(3, DimensionMode::Reject, 3);
    let reference = |x: &[f64]| 2.0 * std::f64::consts::PI * (x[0].powi(3) / x[1]).sqrt();
    let ledger = ws
        .map(|w| w.ledger.clone())
        .unwrap_or_else(simorph_provenance::Ledger::in_memory);
    let d = disc::discover(
        "Kepler",
        &train,
        &held,
        &cfg,
        0.5,
        1e-3,
        Some(("2π√(a³/μ)", &reference, 1e-3)),
        &ledger,
        ctl,
    )?;
    Ok(summarise("troisième loi de Kepler (orbites intégrées)", &d))
}
