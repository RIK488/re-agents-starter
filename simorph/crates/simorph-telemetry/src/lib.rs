//! # simorph-telemetry
//!
//! Observability primitives shared by every SIMORPH service:
//!
//! * [`init`] configures a `tracing` subscriber (text or JSON, env filter).
//! * [`Metrics`] is a thread-safe registry of counters, gauges and
//!   fixed-bucket histograms, exportable as JSON ([`Metrics::snapshot`]) or in
//!   the Prometheus text exposition format ([`Metrics::prometheus`]).
//!
//! OpenTelemetry export is an integration target: the snapshot structure maps
//! one-to-one onto OTLP metric data points, so an OTLP exporter can be added
//! without changing call sites.

use indexmap::IndexMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Once;

/// Output format of the tracing subscriber.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    Text,
    Json,
}

/// Subscriber configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub format: LogFormat,
    /// `tracing_subscriber::EnvFilter` directive, e.g. `"info,simorph=debug"`.
    /// `RUST_LOG` overrides it when set.
    pub filter: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            format: LogFormat::Text,
            filter: "info".into(),
        }
    }
}

static INIT: Once = Once::new();

/// Install the global tracing subscriber.  Idempotent: later calls are
/// ignored (the first configuration wins), so tests and embedded uses can
/// call it freely.
pub fn init(config: &TelemetryConfig) {
    INIT.call_once(|| {
        use tracing_subscriber::{fmt, EnvFilter};
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(config.filter.clone()));
        match config.format {
            LogFormat::Text => {
                let _ = fmt().with_env_filter(filter).try_init();
            }
            LogFormat::Json => {
                let _ = fmt().json().with_env_filter(filter).try_init();
            }
        }
    });
}

/// Label set attached to a metric series.
pub type Labels = BTreeMap<String, String>;

fn series_key(name: &str, labels: &Labels) -> String {
    if labels.is_empty() {
        name.to_string()
    } else {
        let l: Vec<String> = labels.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("{name}{{{}}}", l.join(","))
    }
}

#[derive(Debug, Default)]
struct Counter {
    value: AtomicU64,
}

#[derive(Debug, Default)]
struct Gauge {
    bits: AtomicU64,
}

impl Gauge {
    fn get(&self) -> f64 {
        f64::from_bits(self.bits.load(Ordering::Relaxed))
    }
    fn set(&self, v: f64) {
        self.bits.store(v.to_bits(), Ordering::Relaxed);
    }
}

#[derive(Debug)]
struct Histogram {
    buckets: Vec<f64>,
    counts: Vec<AtomicU64>,
    sum_bits: RwLock<f64>,
    total: AtomicU64,
}

impl Histogram {
    fn new(buckets: Vec<f64>) -> Self {
        let n = buckets.len();
        Self {
            buckets,
            counts: (0..=n).map(|_| AtomicU64::new(0)).collect(),
            sum_bits: RwLock::new(0.0),
            total: AtomicU64::new(0),
        }
    }
    fn observe(&self, v: f64) {
        let idx = self
            .buckets
            .iter()
            .position(|&b| v <= b)
            .unwrap_or(self.buckets.len());
        self.counts[idx].fetch_add(1, Ordering::Relaxed);
        self.total.fetch_add(1, Ordering::Relaxed);
        *self.sum_bits.write() += v;
    }
}

/// Default latency buckets (seconds).
pub const DEFAULT_BUCKETS: [f64; 12] = [
    0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 60.0,
];

/// Snapshot of a histogram series.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistogramSnapshot {
    pub buckets: Vec<f64>,
    /// Cumulative counts per bucket boundary, plus `+Inf` as the last entry.
    pub cumulative_counts: Vec<u64>,
    pub sum: f64,
    pub count: u64,
}

/// JSON-serialisable snapshot of the whole registry.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MetricsSnapshot {
    pub counters: IndexMap<String, u64>,
    pub gauges: IndexMap<String, f64>,
    pub histograms: IndexMap<String, HistogramSnapshot>,
}

/// Thread-safe metrics registry.  Cheap to clone (shared handle).
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    counters: RwLock<IndexMap<String, Arc<Counter>>>,
    gauges: RwLock<IndexMap<String, Arc<Gauge>>>,
    histograms: RwLock<IndexMap<String, Arc<Histogram>>>,
    help: RwLock<IndexMap<String, String>>,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a help string to a metric name (Prometheus `# HELP`).
    pub fn describe(&self, name: &str, help: &str) {
        self.inner
            .help
            .write()
            .insert(name.to_string(), help.to_string());
    }

    pub fn inc(&self, name: &str) {
        self.inc_by(name, &Labels::new(), 1);
    }

    pub fn inc_by(&self, name: &str, labels: &Labels, delta: u64) {
        let key = series_key(name, labels);
        let c = {
            let r = self.inner.counters.read();
            r.get(&key).cloned()
        };
        let c = match c {
            Some(c) => c,
            None => self.inner.counters.write().entry(key).or_default().clone(),
        };
        c.value.fetch_add(delta, Ordering::Relaxed);
    }

    pub fn counter(&self, name: &str, labels: &Labels) -> u64 {
        self.inner
            .counters
            .read()
            .get(&series_key(name, labels))
            .map(|c| c.value.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub fn set_gauge(&self, name: &str, labels: &Labels, value: f64) {
        let key = series_key(name, labels);
        let g = {
            let r = self.inner.gauges.read();
            r.get(&key).cloned()
        };
        let g = match g {
            Some(g) => g,
            None => self.inner.gauges.write().entry(key).or_default().clone(),
        };
        g.set(value);
    }

    pub fn gauge(&self, name: &str, labels: &Labels) -> Option<f64> {
        self.inner
            .gauges
            .read()
            .get(&series_key(name, labels))
            .map(|g| g.get())
    }

    /// Record an observation; the histogram is created with
    /// [`DEFAULT_BUCKETS`] on first use.
    pub fn observe(&self, name: &str, labels: &Labels, value: f64) {
        self.observe_with_buckets(name, labels, value, &DEFAULT_BUCKETS);
    }

    pub fn observe_with_buckets(&self, name: &str, labels: &Labels, value: f64, buckets: &[f64]) {
        let key = series_key(name, labels);
        let h = {
            let r = self.inner.histograms.read();
            r.get(&key).cloned()
        };
        let h = match h {
            Some(h) => h,
            None => self
                .inner
                .histograms
                .write()
                .entry(key)
                .or_insert_with(|| Arc::new(Histogram::new(buckets.to_vec())))
                .clone(),
        };
        h.observe(value);
    }

    /// Time a closure and record its duration (seconds) in a histogram.
    pub fn time<T>(&self, name: &str, labels: &Labels, f: impl FnOnce() -> T) -> T {
        let start = std::time::Instant::now();
        let out = f();
        self.observe(name, labels, start.elapsed().as_secs_f64());
        out
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let mut s = MetricsSnapshot::default();
        for (k, c) in self.inner.counters.read().iter() {
            s.counters
                .insert(k.clone(), c.value.load(Ordering::Relaxed));
        }
        for (k, g) in self.inner.gauges.read().iter() {
            s.gauges.insert(k.clone(), g.get());
        }
        for (k, h) in self.inner.histograms.read().iter() {
            let mut cum = 0u64;
            let mut cumulative = Vec::with_capacity(h.counts.len());
            for c in &h.counts {
                cum += c.load(Ordering::Relaxed);
                cumulative.push(cum);
            }
            s.histograms.insert(
                k.clone(),
                HistogramSnapshot {
                    buckets: h.buckets.clone(),
                    cumulative_counts: cumulative,
                    sum: *h.sum_bits.read(),
                    count: h.total.load(Ordering::Relaxed),
                },
            );
        }
        s
    }

    pub fn snapshot_json(&self) -> serde_json::Value {
        serde_json::to_value(self.snapshot()).expect("snapshot is serialisable")
    }

    /// Prometheus text exposition (version 0.0.4).
    pub fn prometheus(&self) -> String {
        let snap = self.snapshot();
        let help = self.inner.help.read();
        let mut out = String::new();
        let mut typed = std::collections::HashSet::new();
        let split = |key: &str| -> (String, String) {
            match key.find('{') {
                Some(i) => (key[..i].to_string(), key[i..].to_string()),
                None => (key.to_string(), String::new()),
            }
        };
        for (k, v) in &snap.counters {
            let (name, labels) = split(k);
            if typed.insert(name.clone()) {
                if let Some(h) = help.get(&name) {
                    out.push_str(&format!("# HELP {name} {h}\n"));
                }
                out.push_str(&format!("# TYPE {name} counter\n"));
            }
            out.push_str(&format!("{name}{labels} {v}\n"));
        }
        for (k, v) in &snap.gauges {
            let (name, labels) = split(k);
            if typed.insert(name.clone()) {
                if let Some(h) = help.get(&name) {
                    out.push_str(&format!("# HELP {name} {h}\n"));
                }
                out.push_str(&format!("# TYPE {name} gauge\n"));
            }
            out.push_str(&format!("{name}{labels} {v}\n"));
        }
        for (k, h) in &snap.histograms {
            let (name, labels) = split(k);
            if typed.insert(name.clone()) {
                if let Some(hp) = help.get(&name) {
                    out.push_str(&format!("# HELP {name} {hp}\n"));
                }
                out.push_str(&format!("# TYPE {name} histogram\n"));
            }
            let inner = labels.trim_start_matches('{').trim_end_matches('}');
            let with = |extra: &str| -> String {
                if inner.is_empty() {
                    format!("{{{extra}}}")
                } else {
                    format!("{{{inner},{extra}}}")
                }
            };
            for (i, b) in h.buckets.iter().enumerate() {
                out.push_str(&format!(
                    "{name}_bucket{} {}\n",
                    with(&format!("le=\"{b}\"")),
                    h.cumulative_counts[i]
                ));
            }
            out.push_str(&format!(
                "{name}_bucket{} {}\n",
                with("le=\"+Inf\""),
                h.cumulative_counts[h.buckets.len()]
            ));
            out.push_str(&format!("{name}_sum{labels} {}\n", h.sum));
            out.push_str(&format!("{name}_count{labels} {}\n", h.count));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(pairs: &[(&str, &str)]) -> Labels {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn counters_gauges_and_histograms() {
        let m = Metrics::new();
        m.inc("runs_total");
        m.inc_by("runs_total", &labels(&[("component", "orbit")]), 3);
        m.set_gauge("queue_depth", &Labels::new(), 4.5);
        for v in [0.002, 0.02, 0.2, 2.0, 200.0] {
            m.observe("latency_s", &Labels::new(), v);
        }
        assert_eq!(m.counter("runs_total", &Labels::new()), 1);
        assert_eq!(
            m.counter("runs_total", &labels(&[("component", "orbit")])),
            3
        );
        assert_eq!(m.gauge("queue_depth", &Labels::new()), Some(4.5));
        let s = m.snapshot();
        let h = &s.histograms["latency_s"];
        assert_eq!(h.count, 5);
        assert!((h.sum - 202.222).abs() < 1e-9);
        // +Inf bucket is cumulative total
        assert_eq!(*h.cumulative_counts.last().unwrap(), 5);
        // 0.002 <= 0.005 bucket (index 1); 0.001 bucket has zero
        assert_eq!(h.cumulative_counts[0], 0);
        assert_eq!(h.cumulative_counts[1], 1);
    }

    #[test]
    fn prometheus_format() {
        let m = Metrics::new();
        m.describe("runs_total", "Number of runs");
        m.inc_by("runs_total", &labels(&[("component", "orbit")]), 2);
        m.observe_with_buckets("wall_s", &Labels::new(), 0.3, &[0.1, 1.0]);
        let text = m.prometheus();
        assert!(text.contains("# HELP runs_total Number of runs"));
        assert!(text.contains("# TYPE runs_total counter"));
        assert!(text.contains("runs_total{component=orbit} 2"));
        assert!(text.contains("wall_s_bucket{le=\"0.1\"} 0"));
        assert!(text.contains("wall_s_bucket{le=\"1\"} 1"));
        assert!(text.contains("wall_s_bucket{le=\"+Inf\"} 1"));
        assert!(text.contains("wall_s_count 1"));
    }

    #[test]
    fn snapshot_round_trips_json() {
        let m = Metrics::new();
        m.inc("a");
        let json = m.snapshot_json();
        let back: MetricsSnapshot = serde_json::from_value(json).unwrap();
        assert_eq!(back, m.snapshot());
    }

    #[test]
    fn concurrent_increments_are_exact() {
        let m = Metrics::new();
        std::thread::scope(|s| {
            for _ in 0..8 {
                let m = m.clone();
                s.spawn(move || {
                    for _ in 0..1000 {
                        m.inc("hits");
                    }
                });
            }
        });
        assert_eq!(m.counter("hits", &Labels::new()), 8000);
    }

    #[test]
    fn init_is_idempotent() {
        init(&TelemetryConfig::default());
        init(&TelemetryConfig {
            format: LogFormat::Json,
            filter: "debug".into(),
        });
        tracing::info!("telemetry initialised");
    }
}
