//! Instruments: timestamped readings, authorisation-gated setpoints.

use crate::csv_source::CsvSource;
use crate::source::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use simorph_core::{Permission, Quantity, Result, SimorphError, Uncertainty};
use simorph_security::{Grant, HumanApproval, Policy, Principal};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    /// A device reached through a transport (serial line, socket…).
    Physical,
    /// Replays recorded data; never a live measurement.
    Replay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub name: String,
    pub unit: String,
    pub writable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstrumentInfo {
    pub id: String,
    pub kind: InstrumentKind,
    pub identification: String,
    pub channels: Vec<ChannelInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReadingSource {
    Live {
        transport: String,
    },
    Replay {
        file: PathBuf,
        sha256: String,
        row: usize,
        recorded_t_s: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub channel: String,
    pub timestamp: DateTime<Utc>,
    pub quantity: Quantity,
    pub source: ReadingSource,
}

/// Proof that a setpoint write was authorised.  `#[non_exhaustive]` makes it
/// impossible to construct outside this crate: only [`write_setpoint`]
/// creates one, after `Policy::authorize` succeeded.
#[non_exhaustive]
pub struct WriteAuthorization {
    pub grant: Grant,
}

pub trait Instrument: Send {
    fn info(&self) -> InstrumentInfo;
    fn connect(&mut self) -> Result<InstrumentInfo>;
    fn read(&mut self, channel: &str) -> Result<Reading>;
    /// Apply a setpoint (value already in the channel unit).  Requires a
    /// [`WriteAuthorization`], obtainable only through [`write_setpoint`].
    fn apply_setpoint(
        &mut self,
        channel: &str,
        value: f64,
        auth: &WriteAuthorization,
    ) -> Result<()>;
}

/// Authorise and write a setpoint.  Requests `Permission::ExternalSideEffect`
/// for `principal`; an autonomous agent needs a valid human approval.  The
/// value is converted to the channel's unit (dimension checked).
pub fn write_setpoint(
    inst: &mut dyn Instrument,
    channel: &str,
    value: f64,
    unit: &str,
    policy: &Policy,
    principal: &Principal,
    approval: Option<&HumanApproval>,
) -> Result<Grant> {
    let info = inst.info();
    let ch = info
        .channels
        .iter()
        .find(|c| c.name == channel)
        .ok_or_else(|| SimorphError::NotFound(format!("channel `{channel}` on `{}`", info.id)))?;
    if !ch.writable {
        return Err(SimorphError::InvalidInput(format!(
            "channel `{channel}` of `{}` is read-only",
            info.id
        )));
    }
    let v = simorph_units::convert(value, unit, &ch.unit)?;
    let grant = policy.authorize(principal, [Permission::ExternalSideEffect], approval)?;
    let auth = WriteAuthorization {
        grant: grant.clone(),
    };
    inst.apply_setpoint(channel, v, &auth)?;
    Ok(grant)
}

// ------------------------------------------------------------------ replay

/// Replays a recorded CSV (`t [s]` column + one column per channel, optional
/// `<channel>_std` columns).  Every reading is labelled `Replay` with the file
/// hash and row; timestamps are `recorded_at + t`.
pub struct ReplayInstrument {
    id: String,
    path: PathBuf,
    sha256: String,
    recorded_at: DateTime<Utc>,
    time_col: String,
    rows: Vec<Row>,
    schema: Schema,
    cursor: HashMap<String, usize>,
}

impl ReplayInstrument {
    pub fn open(
        id: &str,
        path: impl AsRef<Path>,
        time_col: &str,
        recorded_at: DateTime<Utc>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut src = CsvSource::open(&path)?;
        let schema = src.schema().clone();
        let tc = schema
            .column(time_col)
            .ok_or_else(|| SimorphError::NotFound(format!("time column `{time_col}`")))?;
        if tc.unit.is_none()
            || simorph_units::check_same_dimension(tc.unit.as_deref().unwrap(), "s").is_err()
        {
            return Err(SimorphError::Unit(format!(
                "time column `{time_col}` must have a time unit"
            )));
        }
        let rows = src.read_all(10_000)?;
        Ok(Self {
            id: id.into(),
            sha256: src.provenance().sha256.clone(),
            path,
            recorded_at,
            time_col: time_col.into(),
            rows,
            schema,
            cursor: HashMap::new(),
        })
    }
}

impl Instrument for ReplayInstrument {
    fn info(&self) -> InstrumentInfo {
        let channels = self
            .schema
            .columns
            .iter()
            .filter(|c| {
                c.name != self.time_col
                    && !c.name.ends_with("_std")
                    && c.kind == ColumnKind::Numeric
            })
            .map(|c| ChannelInfo {
                name: c.name.clone(),
                unit: c.unit.clone().unwrap_or_else(|| "1".into()),
                writable: false,
            })
            .collect();
        InstrumentInfo {
            id: self.id.clone(),
            kind: InstrumentKind::Replay,
            identification: format!("REPLAY of {} (sha256 {})", self.path.display(), self.sha256),
            channels,
        }
    }
    fn connect(&mut self) -> Result<InstrumentInfo> {
        Ok(self.info())
    }
    fn read(&mut self, channel: &str) -> Result<Reading> {
        let col = self
            .schema
            .column(channel)
            .filter(|c| c.kind == ColumnKind::Numeric)
            .ok_or_else(|| SimorphError::NotFound(format!("channel `{channel}`")))?
            .clone();
        let tu = self
            .schema
            .column(&self.time_col)
            .and_then(|c| c.unit.clone())
            .unwrap_or_else(|| "s".into());
        let i = self.cursor.entry(channel.to_string()).or_insert(0);
        while *i < self.rows.len()
            && self.rows[*i]
                .get(channel)
                .and_then(|c| c.as_f64())
                .is_none()
        {
            *i += 1;
        }
        if *i >= self.rows.len() {
            return Err(SimorphError::NotFound(format!(
                "replay of channel `{channel}` exhausted"
            )));
        }
        let row = &self.rows[*i];
        let v = row[channel].as_f64().expect("checked");
        let t = simorph_units::convert(
            row.get(&self.time_col)
                .and_then(|c| c.as_f64())
                .unwrap_or(0.0),
            &tu,
            "s",
        )?;
        let std_col = format!("{channel}_std");
        let unc = match row.get(&std_col).and_then(|c| c.as_f64()) {
            Some(sd) => Uncertainty::Normal { std_dev: sd },
            None => Uncertainty::Undeclared,
        };
        let r = Reading {
            channel: channel.into(),
            timestamp: self.recorded_at + chrono::Duration::microseconds((t * 1e6) as i64),
            quantity: Quantity::new(channel, v, col.unit.clone().unwrap_or_else(|| "1".into()))
                .with_uncertainty(unc),
            source: ReadingSource::Replay {
                file: self.path.clone(),
                sha256: self.sha256.clone(),
                row: *i,
                recorded_t_s: t,
            },
        };
        *i += 1;
        Ok(r)
    }
    fn apply_setpoint(
        &mut self,
        channel: &str,
        _value: f64,
        _auth: &WriteAuthorization,
    ) -> Result<()> {
        Err(SimorphError::InvalidInput(format!(
            "`{}` is a replay instrument: channel `{channel}` cannot be written",
            self.id
        )))
    }
}

// ------------------------------------------------------------------ line instrument

/// SCPI-like line protocol over any `Read + Write` transport:
///
/// | request | reply |
/// |---|---|
/// | `*IDN?` | free identification text |
/// | `MEAS:<CH>?` | `<value> <unit>` or `<value> +/- <std> <unit>` |
/// | `SOUR:<CH> <value> <unit>` | `OK` |
/// | anything | `ERR <message>` on failure |
///
/// A serial device is used by opening its device file read/write (line
/// settings configured beforehand, e.g. with `stty`); a TCP instrument by
/// passing a `TcpStream`.
pub struct LineInstrument<T: Read + Write + Send> {
    id: String,
    transport_name: String,
    reader: BufReader<T>,
    channels: Vec<ChannelInfo>,
    identification: String,
}

impl<T: Read + Write + Send> LineInstrument<T> {
    pub fn new(
        id: &str,
        transport_name: &str,
        transport: T,
        channels: Vec<ChannelInfo>,
    ) -> Result<Self> {
        for c in &channels {
            simorph_units::parse_unit(&c.unit)
                .map_err(|e| SimorphError::Unit(format!("channel `{}`: {e}", c.name)))?;
        }
        Ok(Self {
            id: id.into(),
            transport_name: transport_name.into(),
            reader: BufReader::new(transport),
            channels,
            identification: String::new(),
        })
    }

    fn query(&mut self, cmd: &str) -> Result<String> {
        let w = self.reader.get_mut();
        w.write_all(cmd.as_bytes())?;
        w.write_all(b"\n")?;
        w.flush()?;
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(SimorphError::Io(format!("`{}`: transport closed", self.id)));
        }
        let line = line.trim().to_string();
        if let Some(e) = line.strip_prefix("ERR") {
            return Err(SimorphError::Component {
                component: self.id.clone(),
                message: e.trim().to_string(),
            });
        }
        Ok(line)
    }
}

/// Parse `<value> <unit>` or `<value> +/- <std> <unit>` (also `±`).
pub fn parse_measurement(line: &str) -> Result<(f64, Option<f64>, String)> {
    let norm = line.replace('±', " +/- ");
    let parts: Vec<&str> = norm.split_whitespace().collect();
    let bad = || SimorphError::InvalidInput(format!("malformed measurement `{line}`"));
    let v: f64 = parts.first().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    if parts.get(1) == Some(&"+/-") {
        let sd: f64 = parts.get(2).ok_or_else(bad)?.parse().map_err(|_| bad())?;
        Ok((
            v,
            Some(sd),
            parts
                .get(3..)
                .map(|p| p.join(""))
                .unwrap_or_else(|| "1".into()),
        ))
    } else {
        Ok((
            v,
            None,
            parts
                .get(1..)
                .map(|p| p.join(""))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "1".into()),
        ))
    }
}

impl<T: Read + Write + Send> Instrument for LineInstrument<T> {
    fn info(&self) -> InstrumentInfo {
        InstrumentInfo {
            id: self.id.clone(),
            kind: InstrumentKind::Physical,
            identification: self.identification.clone(),
            channels: self.channels.clone(),
        }
    }
    fn connect(&mut self) -> Result<InstrumentInfo> {
        self.identification = self.query("*IDN?")?;
        Ok(self.info())
    }
    fn read(&mut self, channel: &str) -> Result<Reading> {
        let ch = self
            .channels
            .iter()
            .find(|c| c.name == channel)
            .cloned()
            .ok_or_else(|| SimorphError::NotFound(format!("channel `{channel}`")))?;
        let line = self.query(&format!("MEAS:{}?", channel.to_uppercase()))?;
        let (v, sd, unit) = parse_measurement(&line)?;
        // Convert the reported value (and std, a difference) into the declared channel unit.
        let value = simorph_units::convert(v, &unit, &ch.unit)?;
        let scale =
            simorph_units::parse_unit(&unit)?.factor / simorph_units::parse_unit(&ch.unit)?.factor;
        let unc = sd
            .map(|s| Uncertainty::Normal { std_dev: s * scale })
            .unwrap_or(Uncertainty::Undeclared);
        Ok(Reading {
            channel: channel.into(),
            timestamp: Utc::now(),
            quantity: Quantity::new(channel, value, ch.unit.clone()).with_uncertainty(unc),
            source: ReadingSource::Live {
                transport: self.transport_name.clone(),
            },
        })
    }
    fn apply_setpoint(
        &mut self,
        channel: &str,
        value: f64,
        _auth: &WriteAuthorization,
    ) -> Result<()> {
        let ch = self
            .channels
            .iter()
            .find(|c| c.name == channel)
            .cloned()
            .ok_or_else(|| SimorphError::NotFound(format!("channel `{channel}`")))?;
        let reply = self.query(&format!(
            "SOUR:{} {} {}",
            channel.to_uppercase(),
            value,
            ch.unit
        ))?;
        if reply != "OK" {
            return Err(SimorphError::Component {
                component: self.id.clone(),
                message: format!("unexpected reply `{reply}`"),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_security::{AuditLog, Role};
    use std::sync::mpsc;

    /// In-memory full-duplex byte pipe (test transport).
    struct Pipe {
        tx: mpsc::Sender<Vec<u8>>,
        rx: mpsc::Receiver<Vec<u8>>,
        buf: Vec<u8>,
    }
    impl Read for Pipe {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.buf.is_empty() {
                match self.rx.recv() {
                    Ok(b) => self.buf = b,
                    Err(_) => return Ok(0),
                }
            }
            let n = out.len().min(self.buf.len());
            out[..n].copy_from_slice(&self.buf[..n]);
            self.buf.drain(..n);
            Ok(n)
        }
    }
    impl Write for Pipe {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.tx
                .send(b.to_vec())
                .map_err(|_| std::io::Error::other("closed"))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn pipe() -> (Pipe, Pipe) {
        let (a, b) = (mpsc::channel(), mpsc::channel());
        (
            Pipe {
                tx: a.0,
                rx: b.1,
                buf: vec![],
            },
            Pipe {
                tx: b.0,
                rx: a.1,
                buf: vec![],
            },
        )
    }

    /// Test double of a SCPI thermostat — clearly not a real instrument.
    fn fake_device(end: Pipe) {
        std::thread::spawn(move || {
            let mut heater_w = 0.0f64;
            let mut r = BufReader::new(end);
            loop {
                let mut line = String::new();
                if r.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let cmd = line.trim().to_string();
                let reply = if cmd == "*IDN?" {
                    "SIMORPH-TEST-DOUBLE,THERMO-1,0,1.0".to_string()
                } else if cmd == "MEAS:TEMP?" {
                    format!("{} +/- 0.1 degC", 20.0 + heater_w * 0.5)
                } else if cmd == "MEAS:HEATER?" {
                    format!("{heater_w} W")
                } else if let Some(rest) = cmd.strip_prefix("SOUR:HEATER ") {
                    let mut it = rest.split_whitespace();
                    match (it.next().and_then(|v| v.parse::<f64>().ok()), it.next()) {
                        (Some(v), Some("W")) => {
                            heater_w = v;
                            "OK".into()
                        }
                        _ => "ERR bad setpoint".into(),
                    }
                } else {
                    "ERR unknown command".into()
                };
                let w = r.get_mut();
                w.write_all(format!("{reply}\n").as_bytes()).unwrap();
            }
        });
    }

    #[test]
    fn scpi_line_instrument_reads_and_gated_writes() {
        let (host, dev) = pipe();
        fake_device(dev);
        let mut inst = LineInstrument::new(
            "thermo",
            "mem-pipe",
            host,
            vec![
                ChannelInfo {
                    name: "temp".into(),
                    unit: "K".into(),
                    writable: false,
                },
                ChannelInfo {
                    name: "heater".into(),
                    unit: "mW".into(),
                    writable: true,
                },
            ],
        )
        .unwrap();
        let info = inst.connect().unwrap();
        assert!(info.identification.contains("TEST-DOUBLE"));
        let r = inst.read("temp").unwrap();
        assert!((r.quantity.value - 293.15).abs() < 1e-9);
        assert_eq!(r.quantity.uncertainty, Uncertainty::Normal { std_dev: 0.1 });
        // gated write: agent without approval is refused, nothing reaches the device
        let audit = AuditLog::in_memory();
        let policy = Policy::new(b"k".to_vec(), audit.clone());
        let agent = Principal::agent("director");
        assert!(write_setpoint(&mut inst, "heater", 4.0, "W", &policy, &agent, None).is_err());
        assert_eq!(inst.read("heater").unwrap().quantity.value, 0.0);
        // with a human approval the write goes through (converted W → mW → device unit)
        let admin = Principal::admin("alice");
        let ok = HumanApproval::issue(
            &admin,
            &agent,
            [Permission::ExternalSideEffect],
            chrono::Duration::minutes(5),
            b"k",
        )
        .unwrap();
        write_setpoint(&mut inst, "heater", 4.0, "W", &policy, &agent, Some(&ok)).unwrap_err(); // device expects W, channel declared mW: device rejects "4000 mW"
                                                                                                // a researcher (human) holds no ExternalSideEffect by role either
        let res = Principal::new(
            "bob",
            simorph_security::PrincipalKind::User,
            [Role::Researcher],
        )
        .unwrap();
        assert!(write_setpoint(&mut inst, "heater", 4.0, "W", &policy, &res, None).is_err());
        assert!(
            write_setpoint(&mut inst, "temp", 1.0, "K", &policy, &admin, None).is_err(),
            "read-only channel"
        );
        assert!(audit.entries().iter().any(|e| !e.granted));
    }

    #[test]
    fn scpi_write_with_matching_units() {
        let (host, dev) = pipe();
        fake_device(dev);
        let mut inst = LineInstrument::new(
            "thermo",
            "mem-pipe",
            host,
            vec![
                ChannelInfo {
                    name: "temp".into(),
                    unit: "degC".into(),
                    writable: false,
                },
                ChannelInfo {
                    name: "heater".into(),
                    unit: "W".into(),
                    writable: true,
                },
            ],
        )
        .unwrap();
        inst.connect().unwrap();
        let policy = Policy::new(b"k".to_vec(), AuditLog::in_memory());
        let agent = Principal::agent("director");
        let admin = Principal::admin("alice");
        let ok = HumanApproval::issue(
            &admin,
            &agent,
            [Permission::ExternalSideEffect],
            chrono::Duration::minutes(5),
            b"k",
        )
        .unwrap();
        let g = write_setpoint(
            &mut inst,
            "heater",
            4000.0,
            "mW",
            &policy,
            &agent,
            Some(&ok),
        )
        .unwrap();
        assert_eq!(g.approved_by.as_deref(), Some("alice"));
        assert_eq!(inst.read("heater").unwrap().quantity.value, 4.0);
        assert!((inst.read("temp").unwrap().quantity.value - 22.0).abs() < 1e-12);
        // forged approval (wrong secret) refused
        let forged = HumanApproval::issue(
            &admin,
            &agent,
            [Permission::ExternalSideEffect],
            chrono::Duration::minutes(5),
            b"guess",
        )
        .unwrap();
        assert!(write_setpoint(
            &mut inst,
            "heater",
            1.0,
            "W",
            &policy,
            &agent,
            Some(&forged)
        )
        .is_err());
        assert!(parse_measurement("garbage").is_err());
        assert_eq!(
            parse_measurement("1.5 ± 0.2 kPa").unwrap(),
            (1.5, Some(0.2), "kPa".into())
        );
    }

    #[test]
    fn replay_instrument_labels_recorded_data() {
        let d = std::env::temp_dir().join(format!(
            "simorph-conn-replay-{}",
            simorph_core::RunId::new()
        ));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("run.csv");
        std::fs::write(&p, "t [min],temperature [degC],temperature_std [degC],flow [L/min]\n0,20,0.1,1.0\n0.5,21,0.1,\n1,22,0.2,1.2\n").unwrap();
        let t0: DateTime<Utc> = "2026-10-01T08:00:00Z".parse().unwrap();
        let mut r = ReplayInstrument::open("bench-replay", &p, "t", t0).unwrap();
        let info = r.connect().unwrap();
        assert_eq!(info.kind, InstrumentKind::Replay);
        assert!(info.identification.starts_with("REPLAY"));
        assert_eq!(info.channels.len(), 2);
        let a = r.read("temperature").unwrap();
        let b = r.read("temperature").unwrap();
        assert_eq!(b.timestamp, t0 + chrono::Duration::seconds(30));
        assert_eq!(a.quantity.uncertainty, Uncertainty::Normal { std_dev: 0.1 });
        assert!(matches!(b.source, ReadingSource::Replay { row: 1, .. }));
        // flow skips the missing row
        assert_eq!(r.read("flow").unwrap().quantity.value, 1.0);
        assert_eq!(r.read("flow").unwrap().quantity.value, 1.2);
        assert!(r.read("flow").is_err());
        let admin = Principal::admin("alice");
        let policy = Policy::new(b"k".to_vec(), AuditLog::in_memory());
        assert!(write_setpoint(&mut r, "temperature", 1.0, "K", &policy, &admin, None).is_err());
    }
}
