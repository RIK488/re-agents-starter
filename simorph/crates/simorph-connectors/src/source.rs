//! Data-source contract.

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use simorph_core::{Result, SimorphError};
use simorph_provenance::{Ledger, ProvenanceKind, RecordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnKind {
    Numeric,
    Text,
    Bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    /// Validated unit expression; `None` = no unit declared (not the same as "1").
    pub unit: Option<String>,
    pub kind: ColumnKind,
}

impl Column {
    /// Build a column, rejecting unknown unit expressions.
    pub fn new(name: &str, unit: Option<&str>, kind: ColumnKind) -> Result<Self> {
        if let Some(u) = unit {
            simorph_units::parse_unit(u).map_err(|e| {
                SimorphError::Unit(format!("column `{name}`: unknown unit `{u}` ({e})"))
            })?;
        }
        Ok(Self {
            name: name.into(),
            unit: unit.map(|s| s.to_string()),
            kind,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Schema {
    pub columns: Vec<Column>,
}

impl Schema {
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Cell {
    Num(f64),
    Bool(bool),
    Text(String),
    Null,
}

impl Cell {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Cell::Num(x) => Some(*x),
            _ => None,
        }
    }
    pub(crate) fn from_json(v: &serde_json::Value) -> Self {
        match v {
            serde_json::Value::Number(n) => n.as_f64().map(Cell::Num).unwrap_or(Cell::Null),
            serde_json::Value::Bool(b) => Cell::Bool(*b),
            serde_json::Value::String(s) => Cell::Text(s.clone()),
            serde_json::Value::Null => Cell::Null,
            other => Cell::Text(other.to_string()),
        }
    }
}

pub type Row = IndexMap<String, Cell>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub offset: usize,
    pub rows: Vec<Row>,
    /// Offset of the next page, `None` when the source is exhausted.
    pub next_offset: Option<usize>,
    pub total: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub uri: String,
    pub format: String,
    /// SHA-256 of the bytes read (whole file, or concatenated pages).
    pub sha256: String,
    pub retrieved_at: DateTime<Utc>,
    pub rows: Option<usize>,
    /// For paginated remote sources: SHA-256 of each page body.
    #[serde(default)]
    pub page_hashes: Vec<String>,
}

impl SourceProvenance {
    /// Record the source as a `Dataset` node in a provenance ledger.
    pub fn record(&self, ledger: &Ledger, schema: &Schema) -> Result<RecordId> {
        ledger.record(
            ProvenanceKind::Dataset,
            format!("source de données {}", self.uri),
            &serde_json::json!({"provenance": self, "schema": schema}),
            &[],
        )
    }
}

pub trait DataSource {
    fn schema(&self) -> &Schema;
    fn read_page(&mut self, offset: usize, limit: usize) -> Result<Page>;
    fn provenance(&self) -> &SourceProvenance;

    /// Read every row by following pages.
    fn read_all(&mut self, page_size: usize) -> Result<Vec<Row>> {
        let mut out = vec![];
        let mut off = 0;
        loop {
            let p = self.read_page(off, page_size.max(1))?;
            out.extend(p.rows);
            match p.next_offset {
                Some(n) if n > off => off = n,
                _ => break,
            }
        }
        Ok(out)
    }
}

/// Values of a numeric column converted to `target_unit` (dimension checked).
pub fn column_in(
    schema: &Schema,
    rows: &[Row],
    name: &str,
    target_unit: &str,
) -> Result<Vec<Option<f64>>> {
    let col = schema
        .column(name)
        .ok_or_else(|| SimorphError::NotFound(format!("column `{name}`")))?;
    if col.kind != ColumnKind::Numeric {
        return Err(SimorphError::InvalidInput(format!(
            "column `{name}` is not numeric"
        )));
    }
    let from = col.unit.as_deref().ok_or_else(|| {
        SimorphError::Unit(format!(
            "column `{name}` declares no unit; cannot convert to `{target_unit}`"
        ))
    })?;
    let f = simorph_units::parse_unit(from)?;
    let t = simorph_units::parse_unit(target_unit)?;
    if f.dimension != t.dimension {
        return Err(SimorphError::Dimensional(format!(
            "column `{name}` [{from}] cannot be expressed in `{target_unit}`"
        )));
    }
    Ok(rows
        .iter()
        .map(|r| {
            r.get(name)
                .and_then(|c| c.as_f64())
                .map(|v| t.from_si(f.to_si(v)))
        })
        .collect())
}

/// Column name → (SI dimension, values in SI base units).
pub type SiTable = IndexMap<String, (String, Vec<Option<f64>>)>;

/// Every numeric column with a unit, converted to SI base units.
pub fn to_si(schema: &Schema, rows: &[Row]) -> Result<SiTable> {
    let mut out = IndexMap::new();
    for c in schema
        .columns
        .iter()
        .filter(|c| c.kind == ColumnKind::Numeric)
    {
        if let Some(u) = &c.unit {
            let pu = simorph_units::parse_unit(u)?;
            let vals = rows
                .iter()
                .map(|r| r.get(&c.name).and_then(|x| x.as_f64()).map(|v| pu.to_si(v)))
                .collect();
            out.insert(c.name.clone(), (pu.dimension.to_string(), vals));
        }
    }
    Ok(out)
}

/// Infer the kind of a column from its non-null cells.
pub(crate) fn infer_kind(cells: &[&Cell]) -> ColumnKind {
    let non_null: Vec<&&Cell> = cells.iter().filter(|c| !matches!(c, Cell::Null)).collect();
    if !non_null.is_empty() && non_null.iter().all(|c| matches!(c, Cell::Num(_))) {
        ColumnKind::Numeric
    } else if !non_null.is_empty() && non_null.iter().all(|c| matches!(c, Cell::Bool(_))) {
        ColumnKind::Bool
    } else {
        ColumnKind::Text
    }
}

pub(crate) fn paginate(rows: &[Row], offset: usize, limit: usize) -> Page {
    let end = (offset + limit).min(rows.len());
    let slice = if offset < rows.len() {
        rows[offset..end].to_vec()
    } else {
        vec![]
    };
    Page {
        offset,
        rows: slice,
        next_offset: if end < rows.len() { Some(end) } else { None },
        total: Some(rows.len()),
    }
}
