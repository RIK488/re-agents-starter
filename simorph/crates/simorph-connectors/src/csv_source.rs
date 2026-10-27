//! CSV files with units in the header: `temperature [degC],pressure [bar]`.

use crate::source::*;
use chrono::Utc;
use simorph_core::{Result, SimorphError};
use std::path::Path;

pub struct CsvSource {
    schema: Schema,
    rows: Vec<Row>,
    prov: SourceProvenance,
}

/// Split `name [unit]` into (name, unit).
pub fn parse_header(h: &str) -> (String, Option<String>) {
    let h = h.trim();
    if let (Some(open), true) = (h.rfind('['), h.ends_with(']')) {
        let name = h[..open].trim().to_string();
        let unit = h[open + 1..h.len() - 1].trim().to_string();
        return (name, if unit.is_empty() { None } else { Some(unit) });
    }
    (h.to_string(), None)
}

fn parse_cell(s: &str) -> Cell {
    let t = s.trim();
    if t.is_empty()
        || t.eq_ignore_ascii_case("nan")
        || t.eq_ignore_ascii_case("na")
        || t.eq_ignore_ascii_case("null")
    {
        return Cell::Null;
    }
    if let Ok(x) = t.parse::<f64>() {
        return Cell::Num(x);
    }
    match t.to_ascii_lowercase().as_str() {
        "true" => Cell::Bool(true),
        "false" => Cell::Bool(false),
        _ => Cell::Text(t.to_string()),
    }
}

impl CsvSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, b',')
    }

    pub fn open_with(path: impl AsRef<Path>, delimiter: u8) -> Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|e| SimorphError::Io(format!("{}: {e}", path.display())))?;
        Self::from_bytes(&bytes, delimiter, &format!("file://{}", path.display()))
    }

    pub fn from_bytes(bytes: &[u8], delimiter: u8, uri: &str) -> Result<Self> {
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .trim(csv::Trim::All)
            .from_reader(bytes);
        let headers: Vec<(String, Option<String>)> = rdr
            .headers()
            .map_err(|e| SimorphError::InvalidInput(format!("csv header: {e}")))?
            .iter()
            .map(parse_header)
            .collect();
        let mut raw: Vec<Vec<Cell>> = vec![];
        for (i, rec) in rdr.records().enumerate() {
            let rec =
                rec.map_err(|e| SimorphError::InvalidInput(format!("csv row {}: {e}", i + 2)))?;
            if rec.len() != headers.len() {
                return Err(SimorphError::InvalidInput(format!(
                    "csv row {} has {} fields, header has {}",
                    i + 2,
                    rec.len(),
                    headers.len()
                )));
            }
            raw.push(rec.iter().map(parse_cell).collect());
        }
        let mut columns = vec![];
        for (j, (name, unit)) in headers.iter().enumerate() {
            let cells: Vec<&Cell> = raw.iter().map(|r| &r[j]).collect();
            let kind = infer_kind(&cells);
            if unit.is_some() && kind != ColumnKind::Numeric {
                return Err(SimorphError::InvalidInput(format!(
                    "column `{name}` declares unit `{}` but is not numeric",
                    unit.as_deref().unwrap_or("")
                )));
            }
            columns.push(Column::new(name, unit.as_deref(), kind)?);
        }
        let rows: Vec<Row> = raw
            .into_iter()
            .map(|r| {
                columns
                    .iter()
                    .zip(r)
                    .map(|(c, cell)| {
                        let cell = match (c.kind, cell) {
                            (ColumnKind::Text, Cell::Num(x)) => Cell::Text(x.to_string()),
                            (ColumnKind::Text, Cell::Bool(b)) => Cell::Text(b.to_string()),
                            (_, cell) => cell,
                        };
                        (c.name.clone(), cell)
                    })
                    .collect()
            })
            .collect();
        let prov = SourceProvenance {
            uri: uri.into(),
            format: "text/csv".into(),
            sha256: simorph_core::hashing::sha256_hex(bytes),
            retrieved_at: Utc::now(),
            rows: Some(rows.len()),
            page_hashes: vec![],
        };
        Ok(Self {
            schema: Schema { columns },
            rows,
            prov,
        })
    }
}

impl DataSource for CsvSource {
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

    const CSV: &str = "time [s],temperature [degC],pressure [bar],label,ok\n0,20.0,1.0,a,true\n1,21.5,1.2,b,false\n2,,1.4,c,true\n";

    #[test]
    fn units_conversion_pagination_and_provenance() {
        let mut s = CsvSource::from_bytes(CSV.as_bytes(), b',', "mem://t.csv").unwrap();
        let sch = s.schema().clone();
        assert_eq!(
            sch.column("temperature").unwrap().unit.as_deref(),
            Some("degC")
        );
        assert_eq!(sch.column("label").unwrap().kind, ColumnKind::Text);
        assert_eq!(sch.column("ok").unwrap().kind, ColumnKind::Bool);
        let p = s.read_page(0, 2).unwrap();
        assert_eq!(p.rows.len(), 2);
        assert_eq!(p.next_offset, Some(2));
        let all = s.read_all(2).unwrap();
        assert_eq!(all.len(), 3);
        let k = column_in(&sch, &all, "temperature", "K").unwrap();
        assert!((k[1].unwrap() - 294.65).abs() < 1e-9);
        assert_eq!(k[2], None);
        let si = to_si(&sch, &all).unwrap();
        assert_eq!(si["pressure"].1[1], Some(120_000.0));
        assert!(column_in(&sch, &all, "pressure", "m").is_err());
        assert_eq!(
            s.provenance().sha256,
            simorph_core::hashing::sha256_hex(CSV.as_bytes())
        );
        let ledger = simorph_provenance::Ledger::in_memory();
        s.provenance().record(&ledger, &sch).unwrap();
        assert_eq!(ledger.len(), 1);
    }

    #[test]
    fn unknown_units_and_bad_rows_are_rejected() {
        let e = CsvSource::from_bytes(b"x [furlong]\n1\n", b',', "mem://")
            .err()
            .unwrap();
        assert!(e.to_string().contains("furlong"), "{e}");
        assert!(CsvSource::from_bytes(b"a,b\n1\n", b',', "mem://").is_err());
        assert!(CsvSource::from_bytes(b"name [m]\nabc\n", b',', "mem://").is_err());
        let s = CsvSource::from_bytes(b"a [m];b [s]\n1;2\n", b';', "mem://").unwrap();
        assert_eq!(s.schema().columns.len(), 2);
    }
}
