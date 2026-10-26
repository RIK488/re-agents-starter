//! JSON Schema export of the public contracts.

use crate::descriptor::ModelDescriptor;
use simorph_core::{ComponentManifest, ProblemSpec, Result, TaskRequest, TaskResult};
use std::path::{Path, PathBuf};

/// Write the JSON Schemas of the public contracts into `dir`
/// (`component-manifest.schema.json`, `model-descriptor.schema.json`,
/// `problem-spec.schema.json`, `task-request.schema.json`,
/// `task-result.schema.json`).  Returns the written paths.
pub fn export_schemas(dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let dir = dir.as_ref();
    std::fs::create_dir_all(dir)?;
    let entries: Vec<(&str, schemars::schema::RootSchema)> = vec![
        (
            "component-manifest",
            schemars::schema_for!(ComponentManifest),
        ),
        ("model-descriptor", schemars::schema_for!(ModelDescriptor)),
        ("problem-spec", schemars::schema_for!(ProblemSpec)),
        ("task-request", schemars::schema_for!(TaskRequest)),
        ("task-result", schemars::schema_for!(TaskResult)),
    ];
    let mut out = vec![];
    for (name, schema) in entries {
        let path = dir.join(format!("{name}.schema.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&schema)?)?;
        out.push(path);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_writes_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let paths = export_schemas(dir.path()).unwrap();
        assert_eq!(paths.len(), 5);
        for p in paths {
            let text = std::fs::read_to_string(&p).unwrap();
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(v.get("$schema").is_some(), "{p:?} lacks $schema");
            assert!(v.get("title").is_some());
        }
    }
}
