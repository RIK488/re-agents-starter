//! Default component set assembled from the plugins.

use simorph_core::{Component, ComponentManifest, Result};
use simorph_plugin_sdk::ComponentSet;
use std::sync::Arc;

/// All in-process components shipped with SIMORPH.
pub fn default_component_set() -> Result<ComponentSet> {
    let set = ComponentSet::new();
    for m in simorph_plugin_physics::manifests() {
        let id = m.id;
        set.register(move || {
            simorph_plugin_physics::build(id)
                .map(|b| Arc::from(b) as Arc<dyn Component>)
                .ok_or_else(|| {
                    simorph_core::SimorphError::NotFound(format!("physics component {id}"))
                })
        })?;
    }
    for m in simorph_plugin_engineering::manifests() {
        let id = m.id;
        set.register(move || {
            simorph_plugin_engineering::build(id)
                .map(|b| Arc::from(b) as Arc<dyn Component>)
                .ok_or_else(|| {
                    simorph_core::SimorphError::NotFound(format!("engineering component {id}"))
                })
        })?;
    }
    for m in simorph_plugin_ai::manifests() {
        let id = m.id;
        set.register(move || {
            simorph_plugin_ai::build(id)
                .map(|b| Arc::from(b) as Arc<dyn Component>)
                .ok_or_else(|| simorph_core::SimorphError::NotFound(format!("ai component {id}")))
        })?;
    }
    Ok(set)
}

/// Availability of one external scientific engine.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EngineAvailability {
    pub engine: String,
    pub available: bool,
    pub reason: Option<String>,
}

static LAB_SET: std::sync::OnceLock<(ComponentSet, Vec<EngineAvailability>)> =
    std::sync::OnceLock::new();

/// The laboratory component set: native components plus the external
/// scientific engines (SciPy, SymPy, MuJoCo, Gymnasium, PyTorch) running as
/// sandboxed Python components.  Built once per process (engines are
/// started in parallel and kept warm).  `SIMORPH_SCIENTIFIC=0` disables the
/// external engines; an engine whose libraries are missing is reported, not
/// faked.
pub fn component_set() -> Result<ComponentSet> {
    if let Some((s, _)) = LAB_SET.get() {
        return Ok(s.clone());
    }
    let set = default_component_set()?;
    // Multimodal analyses (images, audio, video via ffmpeg, geometry, text).
    for c in simorph_multimodal::components::all()? {
        set.register_instance(Arc::new(c))?;
    }
    let mut report = vec![];
    if std::env::var("SIMORPH_SCIENTIFIC")
        .map(|v| v != "0")
        .unwrap_or(true)
    {
        let handles: Vec<_> = simorph_adapter_scientific_python::ENGINES
            .iter()
            .map(|e| {
                let name = e.name.to_string();
                std::thread::spawn(move || {
                    (
                        name.clone(),
                        simorph_adapter_scientific_python::spawn(&name),
                    )
                })
            })
            .collect();
        for h in handles {
            let (name, res) = h.join().map_err(|_| {
                simorph_core::SimorphError::Other("engine start-up thread panicked".into())
            })?;
            match res {
                Ok(c) => {
                    set.register_instance(c)?;
                    report.push(EngineAvailability {
                        engine: name,
                        available: true,
                        reason: None,
                    });
                }
                Err(e) => report.push(EngineAvailability {
                    engine: name,
                    available: false,
                    reason: Some(e.to_string()),
                }),
            }
        }
    } else {
        report.push(EngineAvailability {
            engine: "*".into(),
            available: false,
            reason: Some("disabled by SIMORPH_SCIENTIFIC=0".into()),
        });
    }
    let _ = LAB_SET.set((set, report));
    Ok(LAB_SET.get().expect("set above").0.clone())
}

/// Availability of the external engines (starts them if needed).
pub fn engine_availability() -> Result<Vec<EngineAvailability>> {
    component_set()?;
    Ok(LAB_SET.get().map(|(_, r)| r.clone()).unwrap_or_default())
}

/// Manifests filtered by optional domain / capability substring.
pub fn list_components(
    set: &ComponentSet,
    domain: Option<&str>,
    capability: Option<&str>,
) -> Vec<ComponentManifest> {
    set.manifests()
        .into_iter()
        .filter(|m| {
            domain
                .map(|d| m.supported_domains.iter().any(|x| x.contains(d)))
                .unwrap_or(true)
        })
        .filter(|m| {
            capability
                .map(|c| {
                    m.capabilities
                        .iter()
                        .any(|x| format!("{x:?}").to_lowercase().contains(&c.to_lowercase()))
                })
                .unwrap_or(true)
        })
        .collect()
}

/// A `simorph_model_registry::Registry` populated with the default set.
pub fn default_registry() -> Result<simorph_model_registry::Registry> {
    let mut r = simorph_model_registry::Registry::new();
    for m in component_set()?.manifests() {
        r.register_manifest(m)?;
    }
    Ok(r)
}
