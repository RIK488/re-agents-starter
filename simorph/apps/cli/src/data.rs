//! Data sources and FMU inspection (subsystems 6 and 19) as CLI / HTTP
//! actions.  Reading a source records it as a `Dataset` node in the
//! workspace provenance ledger (URI, format, SHA-256, row count).

use crate::Workspace;
use serde::{Deserialize, Serialize};
use simorph_connectors::{CsvSource, DataSource, JsonlSource, Row, Schema, SourceProvenance};
use simorph_core::*;
use simorph_provenance::RecordId;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataInspection {
    pub schema: Schema,
    pub provenance: SourceProvenance,
    pub preview: Vec<Row>,
    pub total_rows: Option<usize>,
    /// Provenance record created for this read (when a workspace is given).
    pub recorded: Option<RecordId>,
}

/// Open a CSV (`name [unit]` headers) or JSON-lines file, validate the
/// declared units, and return its schema, fingerprint and first rows.
pub fn inspect_data(ws: Option<&Workspace>, path: &Path, preview: usize) -> Result<DataInspection> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut src: Box<dyn DataSource> = match ext.as_str() {
        "csv" | "tsv" | "txt" => Box::new(if ext == "tsv" {
            CsvSource::open_with(path, b'\t')?
        } else {
            CsvSource::open(path)?
        }),
        "jsonl" | "ndjson" => Box::new(JsonlSource::open(path, Default::default())?),
        other => {
            return Err(SimorphError::InvalidInput(format!(
                "unsupported data format `{other}` (csv, tsv, jsonl)"
            )))
        }
    };
    let page = src.read_page(0, preview.max(1))?;
    let schema = src.schema().clone();
    let provenance = src.provenance().clone();
    let recorded = match ws {
        Some(w) => Some(provenance.record(&w.ledger, &schema)?),
        None => None,
    };
    Ok(DataInspection {
        schema,
        total_rows: page.total.or(provenance.rows),
        provenance,
        preview: page.rows,
        recorded,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FmuInspection {
    pub fmi_version: String,
    pub model_name: String,
    pub guid: String,
    pub cosimulation: bool,
    pub model_exchange: bool,
    pub variables: usize,
    pub unmapped_units: Vec<String>,
    pub manifest: ComponentManifest,
    /// Whether the FMU can be executed here (requires the `fmpy` module).
    pub executable: bool,
    pub execution_note: String,
}

/// Read an FMU's `modelDescription.xml` and derive its component manifest.
pub fn inspect_fmu(path: &Path) -> Result<FmuInspection> {
    let fmu =
        simorph_adapter_fmi::FmuComponent::open(path, simorph_security::SandboxPolicy::strict())?;
    let d = fmu.description();
    let (executable, execution_note) =
        match simorph_adapter_python_bridge::python_module_available("fmpy") {
            Ok(v) => (true, format!("fmpy {v} disponible")),
            Err(e) => (
                false,
                format!("exécution impossible ici : fmpy indisponible ({e})"),
            ),
        };
    Ok(FmuInspection {
        fmi_version: d.fmi_version.clone(),
        model_name: d.model_name.clone(),
        guid: d.guid.clone(),
        cosimulation: d.cosimulation,
        model_exchange: d.model_exchange,
        variables: d.variables.len(),
        unmapped_units: d.unmapped_units.clone(),
        manifest: fmu.manifest().clone(),
        executable,
        execution_note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_is_inspected_with_units_fingerprint_and_provenance() {
        let dir = std::env::temp_dir().join(format!("simorph-data-{}", RunId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("obs.csv");
        std::fs::write(
            &p,
            "diameter [mm],pressure_drop [bar],pressure_drop_std\n200,1.5,0.05\n300,0.21,0.01\n",
        )
        .unwrap();
        let ws = Workspace::open(dir.join("ws")).unwrap();
        let r = inspect_data(Some(&ws), &p, 1).unwrap();
        assert_eq!(r.schema.columns.len(), 3);
        assert_eq!(r.schema.columns[0].unit.as_deref(), Some("mm"));
        assert_eq!(r.preview.len(), 1);
        let digest = {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(std::fs::read(&p).unwrap()))
        };
        assert_eq!(r.provenance.sha256, digest);
        assert!(r.recorded.is_some());
        ws.ledger.verify_chain().unwrap();
        // unknown unit is refused, not guessed
        std::fs::write(&p, "x [furlongs_per_fortnight]\n1\n").unwrap();
        assert!(inspect_data(None, &p, 1).is_err());
        // a non-zip file is not an FMU
        let f = dir.join("x.fmu");
        std::fs::write(&f, b"not a zip").unwrap();
        assert!(inspect_fmu(&f).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
