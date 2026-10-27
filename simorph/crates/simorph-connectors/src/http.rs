//! HTTP sources returning JSON (array of objects, or an object holding the
//! array under `data`, `rows`, `items` or `results`).

use crate::jsonl::from_objects;
use crate::source::*;
use chrono::Utc;
use indexmap::IndexMap;
use simorph_core::{Result, SimorphError};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum Pagination {
    /// One request returns everything; pages are cut locally.
    None,
    /// The server accepts `?<offset_param>=..&<limit_param>=..`.
    OffsetLimit {
        offset_param: String,
        limit_param: String,
    },
}

pub struct HttpJsonSource {
    url: url::Url,
    units: IndexMap<String, String>,
    pagination: Pagination,
    agent: ureq::Agent,
    schema: Schema,
    cache: Option<Vec<Row>>,
    prov: SourceProvenance,
}

fn agent_for(url: &url::Url, timeout: Duration) -> ureq::Agent {
    let loopback = matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
    );
    let b = ureq::AgentBuilder::new().timeout(timeout);
    // Proxies from the environment apply to remote hosts only.
    if loopback {
        b.build()
    } else {
        b.try_proxy_from_env(true).build()
    }
}

fn extract_array(v: serde_json::Value) -> Result<Vec<serde_json::Map<String, serde_json::Value>>> {
    let arr = match v {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(mut o) => {
            let key = ["data", "rows", "items", "results"]
                .into_iter()
                .find(|k| o.get(*k).map(|x| x.is_array()).unwrap_or(false))
                .ok_or_else(|| {
                    SimorphError::InvalidInput(
                        "JSON object has no `data`/`rows`/`items`/`results` array".into(),
                    )
                })?;
            match o.remove(key) {
                Some(serde_json::Value::Array(a)) => a,
                _ => unreachable!(),
            }
        }
        _ => {
            return Err(SimorphError::InvalidInput(
                "response is neither an array nor an object".into(),
            ))
        }
    };
    arr.into_iter()
        .map(|x| match x {
            serde_json::Value::Object(o) => Ok(o),
            _ => Err(SimorphError::InvalidInput(
                "array element is not an object".into(),
            )),
        })
        .collect()
}

impl HttpJsonSource {
    /// Connect and read the first page to establish the schema.
    pub fn new(url: &str, units: IndexMap<String, String>, pagination: Pagination) -> Result<Self> {
        let url =
            url::Url::parse(url).map_err(|e| SimorphError::InvalidInput(format!("url: {e}")))?;
        let agent = agent_for(&url, Duration::from_secs(30));
        let mut s = Self {
            url: url.clone(),
            units,
            pagination,
            agent,
            schema: Schema::default(),
            cache: None,
            prov: SourceProvenance {
                uri: url.to_string(),
                format: "application/json".into(),
                sha256: String::new(),
                retrieved_at: Utc::now(),
                rows: None,
                page_hashes: vec![],
            },
        };
        let first = s.fetch(Some((0, 100)))?;
        let (schema, rows) = from_objects(&first.0, &s.units)?;
        s.schema = schema;
        if s.pagination == Pagination::None {
            s.prov.rows = Some(rows.len());
            s.cache = Some(rows);
        }
        Ok(s)
    }

    fn fetch(
        &mut self,
        window: Option<(usize, usize)>,
    ) -> Result<(Vec<serde_json::Map<String, serde_json::Value>>, String)> {
        let mut u = self.url.clone();
        if let (
            Pagination::OffsetLimit {
                offset_param,
                limit_param,
            },
            Some((o, l)),
        ) = (&self.pagination, window)
        {
            u.query_pairs_mut()
                .append_pair(offset_param, &o.to_string())
                .append_pair(limit_param, &l.to_string());
        }
        let resp = self
            .agent
            .get(u.as_str())
            .call()
            .map_err(|e| SimorphError::Io(format!("GET {u}: {e}")))?;
        let body = resp
            .into_string()
            .map_err(|e| SimorphError::Io(format!("read {u}: {e}")))?;
        let h = simorph_core::hashing::sha256_hex(body.as_bytes());
        self.prov.page_hashes.push(h.clone());
        let joined = self.prov.page_hashes.join("");
        self.prov.sha256 = if self.prov.page_hashes.len() == 1 {
            h.clone()
        } else {
            simorph_core::hashing::sha256_hex(joined.as_bytes())
        };
        self.prov.retrieved_at = Utc::now();
        let v: serde_json::Value = serde_json::from_str(&body)?;
        Ok((extract_array(v)?, h))
    }
}

impl DataSource for HttpJsonSource {
    fn schema(&self) -> &Schema {
        &self.schema
    }
    fn read_page(&mut self, offset: usize, limit: usize) -> Result<Page> {
        if let Some(rows) = &self.cache {
            return Ok(paginate(rows, offset, limit));
        }
        let (objs, _) = self.fetch(Some((offset, limit)))?;
        let (schema, rows) = from_objects(&objs, &self.units)?;
        for c in &schema.columns {
            if let Some(known) = self.schema.column(&c.name) {
                if known.kind != c.kind && !rows.is_empty() {
                    return Err(SimorphError::InvalidInput(format!(
                        "column `{}` changed type between pages",
                        c.name
                    )));
                }
            }
        }
        let n = rows.len();
        Ok(Page {
            offset,
            rows,
            next_offset: if n == limit { Some(offset + n) } else { None },
            total: None,
        })
    }
    fn provenance(&self) -> &SourceProvenance {
        &self.prov
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// Minimal HTTP/1.1 test server: serves `data` with optional offset/limit.
    fn serve(data: Vec<serde_json::Value>, wrap: bool) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut first = String::new();
                r.read_line(&mut first).unwrap();
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).unwrap() == 0 || h == "\r\n" {
                        break;
                    }
                }
                let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
                let q = url::Url::parse(&format!("http://x{path}")).unwrap();
                let get = |k: &str| {
                    q.query_pairs()
                        .find(|(a, _)| a == k)
                        .and_then(|(_, v)| v.parse::<usize>().ok())
                };
                let (o, lim) = (
                    get("offset").unwrap_or(0),
                    get("limit").unwrap_or(usize::MAX),
                );
                let slice: Vec<_> = data.iter().skip(o).take(lim).cloned().collect();
                let body = if wrap {
                    serde_json::json!({"data": slice}).to_string()
                } else {
                    serde_json::Value::Array(slice).to_string()
                };
                let mut s = s;
                write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            }
        });
        format!("http://{addr}/obs")
    }

    #[test]
    fn local_server_paginated_and_unpaginated() {
        let data: Vec<serde_json::Value> = (0..25).map(|i| serde_json::json!({"t": i, "power": 10.0 + i as f64, "station": format!("S{i}")})).collect();
        let units: IndexMap<String, String> = [
            ("t".to_string(), "min".to_string()),
            ("power".to_string(), "kW".to_string()),
        ]
        .into_iter()
        .collect();
        let url = serve(data.clone(), true);
        let mut paged = HttpJsonSource::new(
            &url,
            units.clone(),
            Pagination::OffsetLimit {
                offset_param: "offset".into(),
                limit_param: "limit".into(),
            },
        )
        .unwrap();
        let rows = paged.read_all(10).unwrap();
        assert_eq!(rows.len(), 25);
        let w = column_in(paged.schema(), &rows, "power", "W").unwrap();
        assert_eq!(w[3], Some(13_000.0));
        assert!(paged.provenance().page_hashes.len() >= 4);
        let url2 = serve(data, false);
        let mut whole = HttpJsonSource::new(&url2, units, Pagination::None).unwrap();
        let p = whole.read_page(20, 10).unwrap();
        assert_eq!(p.rows.len(), 5);
        assert_eq!(whole.provenance().rows, Some(25));
        assert!(
            HttpJsonSource::new("http://127.0.0.1:1/none", IndexMap::new(), Pagination::None)
                .is_err()
        );
    }
}
