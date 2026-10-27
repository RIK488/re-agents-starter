//! A trained regressor exposed as a SIMORPH component.

use crate::mlp::Mlp;
use serde::{Deserialize, Serialize};
use simorph_core::*;

/// Feature / target transform applied around the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transform {
    Identity,
    /// `log10(v)`; inputs must be > 0.
    Log10,
}

impl Transform {
    pub fn forward(self, v: f64) -> Result<f64> {
        match self {
            Transform::Identity => Ok(v),
            Transform::Log10 => {
                if v > 0.0 {
                    Ok(v.log10())
                } else {
                    Err(SimorphError::ValidityDomain(format!(
                        "log10 transform needs a positive value, got {v}"
                    )))
                }
            }
        }
    }
    pub fn inverse(self, v: f64) -> f64 {
        match self {
            Transform::Identity => v,
            Transform::Log10 => 10f64.powf(v),
        }
    }
}

/// Specification of the surrogate's interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurrogateSpec {
    pub name: String,
    pub inputs: Vec<(Port, Transform)>,
    pub outputs: Vec<(Port, Transform)>,
    /// What the surrogate approximates (component name), for provenance.
    pub approximates: String,
    /// Hash of the training dataset.
    pub dataset_hash: String,
}

/// Neural surrogate as a `Component`.  Its outputs are always `Simulated`
/// evidence: a learned model never produces computed or validated results by itself.
pub struct SurrogateComponent {
    pub spec: SurrogateSpec,
    pub model: Mlp,
    manifest: ComponentManifest,
}

impl SurrogateComponent {
    pub fn new(spec: SurrogateSpec, model: Mlp, version: &str) -> Result<Self> {
        if model.n_inputs() != spec.inputs.len() || model.n_outputs() != spec.outputs.len() {
            return Err(SimorphError::InvalidInput(
                "surrogate spec does not match the network shape".into(),
            ));
        }
        let mut manifest = ComponentManifest::in_process(&spec.name, version, "simorph-learning")
            .with_capabilities(vec![Capability::Inference])
            .with_domains(&["ai", "surrogate-modelling"])
            .with_inputs(spec.inputs.iter().map(|(p, _)| p.clone()).collect())
            .with_outputs(spec.outputs.iter().map(|(p, _)| p.clone()).collect())
            .with_assumptions(&["neural surrogate: valid only inside the training input domain; error characterised on a held-out suite, not guaranteed"]);
        manifest.id = ComponentId::deterministic(&format!(
            "{}@{}#{}",
            spec.name,
            version,
            model.content_hash()
        ));
        manifest.provenance.generated_by = Some(format!(
            "simorph-learning (approximates {}, dataset {})",
            spec.approximates, spec.dataset_hash
        ));
        manifest.provenance.build_hash = Some(model.content_hash());
        manifest.uncertainty_model = UncertaintyModel::Described {
            description: "empirical error on the sealed evaluation suite (see improvement report)"
                .into(),
        };
        Ok(Self {
            spec,
            model,
            manifest,
        })
    }

    /// Prediction from physical inputs (transforms applied).
    pub fn predict(&self, x: &[f64]) -> Result<Vec<f64>> {
        let xt: Vec<f64> = x
            .iter()
            .zip(&self.spec.inputs)
            .map(|(v, (_, t))| t.forward(*v))
            .collect::<Result<_>>()?;
        Ok(self
            .model
            .predict(&xt)
            .iter()
            .zip(&self.spec.outputs)
            .map(|(v, (_, t))| t.inverse(*v))
            .collect())
    }
}

impl Component for SurrogateComponent {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        ctl.check()?;
        let x: Vec<f64> = self
            .spec
            .inputs
            .iter()
            .map(|(p, _)| req.get_f64(&p.name))
            .collect::<Result<_>>()?;
        let y = self.predict(&x)?;
        let mut r = TaskResult::new(req, EvidenceLevel::Simulated);
        for ((p, _), v) in self.spec.outputs.iter().zip(y) {
            r = r.output(&p.name, v);
        }
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mlp::*;

    #[test]
    fn surrogate_component_learns_power_law_in_log_space() {
        // y = 3 x^2 on [1, 100]: linear in log space
        let x: Vec<Vec<f64>> = (0..40).map(|i| vec![(i as f64 / 39.0) * 2.0]).collect(); // log10 x
        let y: Vec<Vec<f64>> = x
            .iter()
            .map(|v| vec![(3.0f64).log10() + 2.0 * v[0]])
            .collect();
        let mut m = Mlp::new(&[1, 8, 1], Activation::Tanh, Activation::Identity, 1);
        train(
            &mut m,
            &x,
            &y,
            &TrainConfig {
                epochs: 300,
                ..Default::default()
            },
            &ExecutionControl::default(),
        )
        .unwrap();
        let spec = SurrogateSpec {
            name: "test.power".into(),
            inputs: vec![(Port::scalar("x", "1"), Transform::Log10)],
            outputs: vec![(Port::scalar("y", "1"), Transform::Log10)],
            approximates: "y=3x^2".into(),
            dataset_hash: "h".into(),
        };
        let c = SurrogateComponent::new(spec, m, "1").unwrap();
        let r = run_component(
            &c,
            &TaskRequest::new(c.manifest().id).input("x", 10.0),
            &ExecutionControl::default(),
        )
        .unwrap();
        let y = r.get_f64("y").unwrap();
        assert!((y - 300.0).abs() / 300.0 < 0.02, "{y}");
        assert_eq!(r.evidence, EvidenceLevel::Simulated);
        assert!(c
            .manifest()
            .provenance
            .generated_by
            .as_ref()
            .unwrap()
            .contains("simorph-learning"));
        assert!(run_component(
            &c,
            &TaskRequest::new(c.manifest().id).input("x", -1.0),
            &ExecutionControl::default()
        )
        .is_err());
    }
}
