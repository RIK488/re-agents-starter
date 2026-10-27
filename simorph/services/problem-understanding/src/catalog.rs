//! Port catalogue built from component manifests.

use simorph_core::ComponentManifest;
use simorph_units::Dimension;

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub name: String,
    pub unit: String,
    pub dimension: Option<Dimension>,
    pub input: bool,
    pub components: Vec<String>,
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub ports: Vec<PortInfo>,
}

impl Catalog {
    pub fn from_manifests(ms: &[ComponentManifest]) -> Self {
        let mut ports: Vec<PortInfo> = vec![];
        for m in ms {
            for (input, list) in [(true, &m.input_schema), (false, &m.output_schema)] {
                for p in list.iter() {
                    let dim = simorph_units::parse_unit(&p.unit).ok().map(|u| u.dimension);
                    if let Some(e) = ports
                        .iter_mut()
                        .find(|e| e.name == p.name && e.input == input && e.dimension == dim)
                    {
                        if !e.components.contains(&m.name) {
                            e.components.push(m.name.clone());
                        }
                        for d in &m.supported_domains {
                            if !e.domains.contains(d) {
                                e.domains.push(d.clone());
                            }
                        }
                    } else {
                        ports.push(PortInfo {
                            name: p.name.clone(),
                            unit: p.unit.clone(),
                            dimension: dim,
                            input,
                            components: vec![m.name.clone()],
                            domains: m.supported_domains.clone(),
                        });
                    }
                }
            }
        }
        Self { ports }
    }

    pub fn port(&self, name: &str, input: bool) -> Option<&PortInfo> {
        self.ports
            .iter()
            .find(|p| p.name == name && p.input == input)
    }

    pub fn has_port(&self, name: &str) -> bool {
        self.ports.iter().any(|p| p.name == name)
    }

    /// Ports compatible with a role and (optionally) a dimension.
    pub fn candidates(&self, input: bool, dim: Option<Dimension>) -> Vec<&PortInfo> {
        self.ports
            .iter()
            .filter(|p| p.input == input)
            .filter(|p| match (dim, p.dimension) {
                (Some(d), Some(pd)) => d == pd,
                (Some(_), None) => false,
                (None, _) => true,
            })
            .collect()
    }
}
