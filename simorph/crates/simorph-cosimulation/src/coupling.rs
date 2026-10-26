//! Coupling description: slaves, connections, unit conversion and
//! dependency analysis.

use crate::slave::{expand_port, find_port, CosimSlave};
use simorph_core::{Result, SimorphError};
use simorph_simulation::Params;
use simorph_units::{parse_unit, Unit};

/// A directed signal connection with precomputed unit conversion.
#[derive(Debug, Clone)]
pub struct Connection {
    pub from_slave: usize,
    pub from_port: String,
    pub to_slave: usize,
    pub to_port: String,
    pub from_unit: Unit,
    pub to_unit: Unit,
}

impl Connection {
    /// Convert a value from the source unit to the target unit.
    pub fn convert(&self, v: f64) -> f64 {
        self.to_unit.from_si(self.from_unit.to_si(v))
    }
}

pub struct SlaveEntry {
    pub name: String,
    pub slave: Box<dyn CosimSlave>,
    pub params: Params,
    /// Scalar output names (vector ports expanded).
    pub output_names: Vec<String>,
}

/// A built coupling, ready for a master algorithm.
pub struct Coupling {
    pub slaves: Vec<SlaveEntry>,
    pub connections: Vec<Connection>,
    /// Stepping order for sequential masters.
    pub order: Vec<usize>,
    /// Algebraic loops (groups of slave names) detected among
    /// direct-feedthrough slaves.
    pub algebraic_loops: Vec<Vec<String>>,
}

impl Coupling {
    pub fn builder() -> CouplingBuilder {
        CouplingBuilder::default()
    }
    pub fn slave_index(&self, name: &str) -> Option<usize> {
        self.slaves.iter().position(|s| s.name == name)
    }
    pub fn has_algebraic_loops(&self) -> bool {
        !self.algebraic_loops.is_empty()
    }
    /// All recorded signal names, `"slave.port"`.
    pub fn signal_names(&self) -> Vec<String> {
        self.slaves
            .iter()
            .flat_map(|s| {
                s.output_names
                    .iter()
                    .map(move |o| format!("{}.{}", s.name, o))
            })
            .collect()
    }
}

#[derive(Default)]
pub struct CouplingBuilder {
    slaves: Vec<(String, Box<dyn CosimSlave>, Params)>,
    connections: Vec<Connection>,
}

fn split_ref(s: &str) -> Result<(&str, &str)> {
    s.split_once('.').ok_or_else(|| {
        SimorphError::InvalidInput(format!("port reference `{s}` must be `slave.port`"))
    })
}

impl CouplingBuilder {
    pub fn add_slave(
        mut self,
        name: &str,
        slave: Box<dyn CosimSlave>,
        params: Params,
    ) -> Result<Self> {
        if self.slaves.iter().any(|(n, _, _)| n == name) {
            return Err(SimorphError::InvalidInput(format!(
                "duplicate slave name `{name}`"
            )));
        }
        slave.manifest().validate_structure()?;
        self.slaves.push((name.to_string(), slave, params));
        Ok(self)
    }

    /// Connect `"a.out"` to `"b.in"`.  Fails if a port is unknown or the
    /// units are dimensionally incompatible.
    pub fn connect(mut self, from: &str, to: &str) -> Result<Self> {
        let (fs, fp) = split_ref(from)?;
        let (ts, tp) = split_ref(to)?;
        let fi = self
            .slaves
            .iter()
            .position(|(n, _, _)| n == fs)
            .ok_or_else(|| SimorphError::NotFound(format!("slave `{fs}`")))?;
        let ti = self
            .slaves
            .iter()
            .position(|(n, _, _)| n == ts)
            .ok_or_else(|| SimorphError::NotFound(format!("slave `{ts}`")))?;
        let fport = find_port(&self.slaves[fi].1.manifest().output_schema, fp)
            .ok_or_else(|| SimorphError::NotFound(format!("output port `{fp}` on slave `{fs}`")))?;
        let tport = find_port(&self.slaves[ti].1.manifest().input_schema, tp)
            .ok_or_else(|| SimorphError::NotFound(format!("input port `{tp}` on slave `{ts}`")))?;
        let fu = parse_unit(&fport.unit)?;
        let tu = parse_unit(&tport.unit)?;
        if fu.dimension != tu.dimension {
            return Err(SimorphError::Dimensional(format!(
                "cannot connect `{from}` [{}] to `{to}` [{}]",
                fu.dimension, tu.dimension
            )));
        }
        if self
            .connections
            .iter()
            .any(|c| c.to_slave == ti && c.to_port == tp)
        {
            return Err(SimorphError::InvalidInput(format!(
                "input `{to}` is already connected"
            )));
        }
        self.connections.push(Connection {
            from_slave: fi,
            from_port: fp.to_string(),
            to_slave: ti,
            to_port: tp.to_string(),
            from_unit: fu,
            to_unit: tu,
        });
        Ok(self)
    }

    pub fn build(self) -> Result<Coupling> {
        let n = self.slaves.len();
        if n == 0 {
            return Err(SimorphError::InvalidInput("coupling has no slaves".into()));
        }
        let feedthrough: Vec<bool> = self
            .slaves
            .iter()
            .map(|(_, s, _)| s.has_direct_feedthrough())
            .collect();
        let full: Vec<Vec<usize>> = (0..n)
            .map(|i| {
                let mut v: Vec<usize> = self
                    .connections
                    .iter()
                    .filter(|c| c.from_slave == i && c.to_slave != i)
                    .map(|c| c.to_slave)
                    .collect();
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect();
        let names: Vec<String> = self.slaves.iter().map(|(n, _, _)| n.clone()).collect();
        // Try a topological order on the full graph first.
        let (order, loops) = match topo_sort(&full) {
            Some(o) => (o, vec![]),
            None => {
                // Inside each strongly connected component, drop the edges
                // leaving non-feedthrough slaves (their outputs are
                // state-based, so a one-macro-step lag on those signals is
                // the standard non-iterative co-simulation behaviour).  What
                // remains cyclic is an algebraic loop.
                let sccs_full = tarjan_scc(&full);
                let mut comp = vec![0usize; n];
                for (k, c) in sccs_full.iter().enumerate() {
                    for &v in c {
                        comp[v] = k;
                    }
                }
                let reduced: Vec<Vec<usize>> = (0..n)
                    .map(|u| {
                        full[u]
                            .iter()
                            .copied()
                            .filter(|&w| comp[u] != comp[w] || feedthrough[u])
                            .collect()
                    })
                    .collect();
                let sccs = tarjan_scc(&reduced);
                let loops: Vec<Vec<String>> = sccs
                    .iter()
                    .filter(|c| c.len() > 1)
                    .map(|c| {
                        let mut c = c.clone();
                        c.sort_unstable();
                        c.iter().map(|&i| names[i].clone()).collect()
                    })
                    .collect();
                // Tarjan yields SCCs in reverse topological order.
                let mut order: Vec<usize> = vec![];
                for c in sccs.iter().rev() {
                    let mut c = c.clone();
                    c.sort_unstable();
                    order.extend(c);
                }
                (order, loops)
            }
        };
        let slaves = self
            .slaves
            .into_iter()
            .map(|(name, slave, params)| {
                let output_names = slave
                    .manifest()
                    .output_schema
                    .iter()
                    .flat_map(expand_port)
                    .collect();
                SlaveEntry {
                    name,
                    slave,
                    params,
                    output_names,
                }
            })
            .collect();
        Ok(Coupling {
            slaves,
            connections: self.connections,
            order,
            algebraic_loops: loops,
        })
    }
}

fn topo_sort(adj: &[Vec<usize>]) -> Option<Vec<usize>> {
    let n = adj.len();
    let mut indeg = vec![0usize; n];
    for v in adj {
        for &j in v {
            indeg[j] += 1;
        }
    }
    let mut ready: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    ready.reverse();
    let mut out = vec![];
    while let Some(i) = ready.pop() {
        out.push(i);
        for &j in &adj[i] {
            indeg[j] -= 1;
            if indeg[j] == 0 {
                ready.push(j);
                ready.sort_unstable_by(|a, b| b.cmp(a));
            }
        }
    }
    if out.len() == n {
        Some(out)
    } else {
        None
    }
}

fn tarjan_scc(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct St<'a> {
        adj: &'a [Vec<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        out: Vec<Vec<usize>>,
    }
    fn visit(s: &mut St, v: usize) {
        s.index[v] = Some(s.next);
        s.low[v] = s.next;
        s.next += 1;
        s.stack.push(v);
        s.on[v] = true;
        for &w in &s.adj[v].to_vec() {
            if s.index[w].is_none() {
                visit(s, w);
                s.low[v] = s.low[v].min(s.low[w]);
            } else if s.on[w] {
                s.low[v] = s.low[v].min(s.index[w].unwrap());
            }
        }
        if s.low[v] == s.index[v].unwrap() {
            let mut comp = vec![];
            loop {
                let w = s.stack.pop().unwrap();
                s.on[w] = false;
                comp.push(w);
                if w == v {
                    break;
                }
            }
            s.out.push(comp);
        }
    }
    let n = adj.len();
    let mut s = St {
        adj,
        index: vec![None; n],
        low: vec![0; n],
        on: vec![false; n],
        stack: vec![],
        next: 0,
        out: vec![],
    };
    for v in 0..n {
        if s.index[v].is_none() {
            visit(&mut s, v);
        }
    }
    s.out
}
