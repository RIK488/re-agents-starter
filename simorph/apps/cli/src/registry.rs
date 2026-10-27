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
    for m in default_component_set()?.manifests() {
        r.register_manifest(m)?;
    }
    Ok(r)
}
