//! JSON-lines files with units declared by the caller.

use crate::source::*;
use chrono::Utc;
use indexmap::IndexMap;
use simorph_core::{Result, SimorphError};
use std::path::Path;

pub struct JsonlSource {
    schema: Schema,
    rows: Vec<Row>,
    prov: SourceProvenance,
}

/// Build schema + rows from JSON objects; `units` maps column → unit.
pub(crate) fn from_objects(
    objs: &[serde_json::Map<String, serde_json::Value>],
    units: &IndexMap<String, String>,
) -> Result<(Schema, Vec<Row>)> {
    let mut names: Vec<String> = vec![];
    for o in objs {
        for k in o.keys() {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
    }
    for u in units.keys() {
        if !names.contains(u) {
            return Err(SimorphError::InvalidInput(format!(
                "unit declared for absent column `{u}`"
            )));
        }
    }
    let rows: Vec<Row> = objs
        .iter()
        .map(|o| {
            names
                .iter()
                .map(|n| {
                    (
                        n.clone(),
                        o.get(n).map(Cell::from_json).unwrap_or(Cell::Null),
                    )
                })
                .collect()
        })
        .collect();
    let mut columns = vec![];
    for n in &names {
        let cells: Vec<&Cell> = rows.iter().map(|r| &r[n]).collect();
        let kind = infer_kind(&cells);
        let unit = units.get(n).map(|s| s.as_str());
        if unit.is_some() && kind != ColumnKind::Numeric {
            return Err(SimorphError::InvalidInput(format!(
                "unit declared for non-numeric column `{n}`"
            )));
        }
        columns.push(Column::new(n, unit, kind)?);
    }
    Ok((Schema { columns }, rows))
}

impl JsonlSource {
    pub fn open(path: impl AsRef<Path>, units: IndexMap<String, String>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|e| SimorphError::Io(format!("{}: {e}", path.display())))?;
        Self::from_bytes(&bytes, units, &format!("file://{}", path.display()))
    }

    pub fn from_bytes(bytes: &[u8], units: IndexMap<String, String>, uri: &str) -> Result<Self> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| SimorphError::InvalidInput("JSON-lines file is not UTF-8".into()))?;
        let mut objs = vec![];
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<serde_json::Value>(line)
                .map_err(|e| SimorphError::InvalidInput(format!("line {}: {e}", i + 1)))?
            {
                serde_json::Value::Object(o) => objs.push(o),
                _ => {
                    return Err(SimorphError::InvalidInput(format!(
                        "line {}: not a JSON object",
                        i + 1
                    )))
                }
            }
        }
        let (schema, rows) = from_objects(&objs, &units)?;
        let prov = SourceProvenance {
            uri: uri.into(),
            format: "application/jsonl".into(),
            sha256: simorph_core::hashing::sha256_hex(bytes),
            retrieved_at: Utc::now(),
            rows: Some(rows.len()),
            page_hashes: vec![],
        };
        Ok(Self { schema, rows, prov })
    }
}

impl DataSource for JsonlSource {
    fn schema(&self) -> &Schema {
        &self.schema
    }
    fn read_page(&mut self, offset: usize, limit: usize) -> Result<Page> {
        Ok(paginate(&self.rows, offset, limit))
    }
    fn provenance(&self) -> &SourceProvenance {
        &self.prov
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonl_units_and_missing_fields() {
        let data =
            b"{\"h\": 500, \"site\": \"A\"}\n{\"h\": 0.6, \"site\": \"B\", \"extra\": true}\n";
        let units: IndexMap<String, String> =
            [("h".to_string(), "km".to_string())].into_iter().collect();
        let mut s = JsonlSource::from_bytes(data, units, "mem://").unwrap();
        let rows = s.read_all(10).unwrap();
        assert_eq!(rows[0]["extra"], Cell::Null);
        let m = column_in(s.schema(), &rows, "h", "m").unwrap();
        assert_eq!(m, vec![Some(500_000.0), Some(600.0)]);
        let bad: IndexMap<String, String> = [("h".to_string(), "parsec_per_fortnight".to_string())]
            .into_iter()
            .collect();
        assert!(JsonlSource::from_bytes(data, bad, "mem://").is_err());
        assert!(JsonlSource::from_bytes(b"[1,2]\n", IndexMap::new(), "mem://").is_err());
    }
}
