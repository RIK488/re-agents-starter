//! Component wrapper owned by the factory: carries the documented manifest
//! and injects the selected solver configuration.

use simorph_core::{
    Component, ComponentManifest, ExecutionControl, Result, TaskRequest, TaskResult,
};
use std::sync::Arc;

pub struct FactoryComponent {
    inner: Arc<dyn Component>,
    manifest: ComponentManifest,
    default_config: Option<serde_json::Value>,
}

impl FactoryComponent {
    pub fn new(inner: Arc<dyn Component>, manifest: ComponentManifest) -> Self {
        Self {
            inner,
            manifest,
            default_config: None,
        }
    }
    pub fn with_default_config(mut self, cfg: serde_json::Value) -> Self {
        self.default_config = Some(cfg);
        self
    }
    pub fn set_manifest(&mut self, m: ComponentManifest) {
        self.manifest = m;
    }
    pub fn manifest_mut(&mut self) -> &mut ComponentManifest {
        &mut self.manifest
    }
    pub fn inner(&self) -> &Arc<dyn Component> {
        &self.inner
    }
    pub fn default_config(&self) -> Option<&serde_json::Value> {
        self.default_config.as_ref()
    }
}

impl Component for FactoryComponent {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }
    fn check_compatibility(&self, req: &TaskRequest) -> simorph_core::Compatibility {
        self.inner.check_compatibility(req)
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        let mut req = req.clone();
        req.component = self.inner.manifest().id;
        if req.config.is_null() {
            if let Some(c) = &self.default_config {
                req.config = c.clone();
            }
        }
        let mut res = self.inner.execute(&req, ctl)?;
        res.component = self.manifest.id;
        Ok(res)
    }
}
