//! Triangle meshes: STL I/O and mass properties.

use crate::asset::{Analysis, MediaAsset, MediaKind};
use serde::{Deserialize, Serialize};
use simorph_core::{Quantity, Result, SimorphError, Uncertainty};
use std::collections::HashMap;
use std::path::Path;

pub type V3 = [f64; 3];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    pub triangles: Vec<[V3; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Topology {
    pub vertices: usize,
    pub edges: usize,
    /// Edges used by exactly one triangle (holes).
    pub boundary_edges: usize,
    /// Edges used by more than two triangles.
    pub nonmanifold_edges: usize,
    /// Edges traversed twice in the same direction (inconsistent orientation).
    pub orientation_conflicts: usize,
    pub watertight: bool,
    /// V − E + F (2 for a closed genus-0 surface).
    pub euler_characteristic: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassProperties {
    /// Signed volume (negative when normals point inwards).
    pub volume: f64,
    pub surface_area: f64,
    pub mass: f64,
    pub centroid: V3,
    /// Inertia tensor about the centroid, row-major.
    pub inertia: [[f64; 3]; 3],
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

impl Mesh {
    pub fn parse_stl(bytes: &[u8]) -> Result<Self> {
        let binary_len_ok = bytes.len() >= 84 && {
            let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
            84 + 50 * n == bytes.len()
        };
        if binary_len_ok {
            let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
            let f = |o: usize| {
                f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]) as f64
            };
            let mut tris = Vec::with_capacity(n);
            for i in 0..n {
                let o = 84 + 50 * i + 12;
                let v = |k: usize| [f(o + 12 * k), f(o + 12 * k + 4), f(o + 12 * k + 8)];
                tris.push([v(0), v(1), v(2)]);
            }
            return Ok(Self { triangles: tris });
        }
        let text = std::str::from_utf8(bytes).map_err(|_| {
            SimorphError::InvalidInput("STL is neither valid binary nor ASCII".into())
        })?;
        let mut verts = vec![];
        for (ln, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("vertex") {
                let c: Vec<f64> = rest
                    .split_whitespace()
                    .map(|x| x.parse::<f64>())
                    .collect::<std::result::Result<_, _>>()
                    .map_err(|e| SimorphError::InvalidInput(format!("STL line {}: {e}", ln + 1)))?;
                if c.len() != 3 {
                    return Err(SimorphError::InvalidInput(format!(
                        "STL line {}: vertex needs 3 coordinates",
                        ln + 1
                    )));
                }
                verts.push([c[0], c[1], c[2]]);
            }
        }
        if verts.len() % 3 != 0 || verts.is_empty() {
            return Err(SimorphError::InvalidInput(format!(
                "ASCII STL has {} vertices (not a positive multiple of 3)",
                verts.len()
            )));
        }
        Ok(Self {
            triangles: verts.chunks(3).map(|c| [c[0], c[1], c[2]]).collect(),
        })
    }

    pub fn to_binary_stl(&self) -> Vec<u8> {
        let mut b = vec![0u8; 80];
        b[..19].copy_from_slice(b"simorph binary stl ");
        b.extend_from_slice(&(self.triangles.len() as u32).to_le_bytes());
        for t in &self.triangles {
            let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            let l = norm(n).max(1e-300);
            for c in [n[0] / l, n[1] / l, n[2] / l] {
                b.extend_from_slice(&(c as f32).to_le_bytes());
            }
            for v in t {
                for c in v {
                    b.extend_from_slice(&(*c as f32).to_le_bytes());
                }
            }
            b.extend_from_slice(&0u16.to_le_bytes());
        }
        b
    }

    pub fn to_ascii_stl(&self, name: &str) -> String {
        let mut s = format!("solid {name}\n");
        for t in &self.triangles {
            let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            let l = norm(n).max(1e-300);
            s.push_str(&format!(
                "  facet normal {:e} {:e} {:e}\n    outer loop\n",
                n[0] / l,
                n[1] / l,
                n[2] / l
            ));
            for v in t {
                s.push_str(&format!("      vertex {:e} {:e} {:e}\n", v[0], v[1], v[2]));
            }
            s.push_str("    endloop\n  endfacet\n");
        }
        s.push_str(&format!("endsolid {name}\n"));
        s
    }

    pub fn scaled(&self, k: f64) -> Self {
        Self {
            triangles: self
                .triangles
                .iter()
                .map(|t| t.map(|v| [v[0] * k, v[1] * k, v[2] * k]))
                .collect(),
        }
    }

    /// Mass properties for a homogeneous solid of `density`
    /// (Eberly, "Polyhedral Mass Properties (Revisited)").
    pub fn mass_properties(&self, density: f64) -> MassProperties {
        let mut i = [0.0f64; 10];
        let mut area = 0.0;
        fn sx(w0: f64, w1: f64, w2: f64) -> (f64, f64, f64, f64, f64, f64) {
            let t0 = w0 + w1;
            let f1 = t0 + w2;
            let t1 = w0 * w0;
            let t2 = t1 + w1 * t0;
            let f2 = t2 + w2 * f1;
            let f3 = w0 * t1 + w1 * t2 + w2 * f2;
            (
                f1,
                f2,
                f3,
                f2 + w0 * (f1 + w0),
                f2 + w1 * (f1 + w1),
                f2 + w2 * (f1 + w2),
            )
        }
        for t in &self.triangles {
            let (p0, p1, p2) = (t[0], t[1], t[2]);
            let d = cross(sub(p1, p0), sub(p2, p0));
            area += 0.5 * norm(d);
            let (f1x, f2x, f3x, g0x, g1x, g2x) = sx(p0[0], p1[0], p2[0]);
            let (_f1y, f2y, f3y, g0y, g1y, g2y) = sx(p0[1], p1[1], p2[1]);
            let (_f1z, f2z, f3z, g0z, g1z, g2z) = sx(p0[2], p1[2], p2[2]);
            i[0] += d[0] * f1x;
            i[1] += d[0] * f2x;
            i[2] += d[1] * f2y;
            i[3] += d[2] * f2z;
            i[4] += d[0] * f3x;
            i[5] += d[1] * f3y;
            i[6] += d[2] * f3z;
            i[7] += d[0] * (p0[1] * g0x + p1[1] * g1x + p2[1] * g2x);
            i[8] += d[1] * (p0[2] * g0y + p1[2] * g1y + p2[2] * g2y);
            i[9] += d[2] * (p0[0] * g0z + p1[0] * g1z + p2[0] * g2z);
        }
        let m = [
            1.0 / 6.0,
            1.0 / 24.0,
            1.0 / 24.0,
            1.0 / 24.0,
            1.0 / 60.0,
            1.0 / 60.0,
            1.0 / 60.0,
            1.0 / 120.0,
            1.0 / 120.0,
            1.0 / 120.0,
        ];
        for k in 0..10 {
            i[k] *= m[k];
        }
        let volume = i[0];
        let c = if volume.abs() > 0.0 {
            [i[1] / volume, i[2] / volume, i[3] / volume]
        } else {
            [f64::NAN; 3]
        };
        let xx = i[5] + i[6] - volume * (c[1] * c[1] + c[2] * c[2]);
        let yy = i[4] + i[6] - volume * (c[2] * c[2] + c[0] * c[0]);
        let zz = i[4] + i[5] - volume * (c[0] * c[0] + c[1] * c[1]);
        let xy = -(i[7] - volume * c[0] * c[1]);
        let yz = -(i[8] - volume * c[1] * c[2]);
        let xz = -(i[9] - volume * c[2] * c[0]);
        let s = density;
        MassProperties {
            volume,
            surface_area: area,
            mass: volume * density,
            centroid: c,
            inertia: [
                [xx * s, xy * s, xz * s],
                [xy * s, yy * s, yz * s],
                [xz * s, yz * s, zz * s],
            ],
        }
    }

    /// Weld vertices (tolerance relative to the bounding box) and check
    /// edge manifoldness and orientation consistency.
    pub fn topology(&self) -> Topology {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for t in &self.triangles {
            for v in t {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
        }
        let diag = norm(sub(hi, lo)).max(1e-300);
        let q = diag * 1e-9;
        let mut ids: HashMap<[i64; 3], usize> = HashMap::new();
        let mut key = |v: V3| -> usize {
            let k = [
                (v[0] / q).round() as i64,
                (v[1] / q).round() as i64,
                (v[2] / q).round() as i64,
            ];
            let n = ids.len();
            *ids.entry(k).or_insert(n)
        };
        let mut directed: HashMap<(usize, usize), usize> = HashMap::new();
        let mut faces = 0i64;
        for t in &self.triangles {
            let a = [key(t[0]), key(t[1]), key(t[2])];
            if a[0] == a[1] || a[1] == a[2] || a[0] == a[2] {
                continue; // degenerate
            }
            faces += 1;
            for e in [(a[0], a[1]), (a[1], a[2]), (a[2], a[0])] {
                *directed.entry(e).or_insert(0) += 1;
            }
        }
        let mut undirected: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
        for (&(a, b), &n) in &directed {
            let k = (a.min(b), a.max(b));
            let e = undirected.entry(k).or_insert((0, 0));
            if a < b {
                e.0 += n
            } else {
                e.1 += n
            }
        }
        let (mut boundary, mut nonmanifold, mut conflicts) = (0, 0, 0);
        for &(f, r) in undirected.values() {
            match f + r {
                1 => boundary += 1,
                2 => {
                    if f != 1 {
                        conflicts += 1
                    }
                }
                _ => nonmanifold += 1,
            }
        }
        let v = ids.len() as i64;
        let e = undirected.len() as i64;
        Topology {
            vertices: ids.len(),
            edges: undirected.len(),
            boundary_edges: boundary,
            nonmanifold_edges: nonmanifold,
            orientation_conflicts: conflicts,
            watertight: boundary == 0 && nonmanifold == 0 && conflicts == 0,
            euler_characteristic: v - e + faces,
        }
    }
}

/// Axis-aligned box with outward normals.
pub fn box_mesh(a: f64, b: f64, c: f64) -> Mesh {
    let p = |x: f64, y: f64, z: f64| [x * a, y * b, z * c];
    let v = [
        p(0., 0., 0.),
        p(1., 0., 0.),
        p(1., 1., 0.),
        p(0., 1., 0.),
        p(0., 0., 1.),
        p(1., 0., 1.),
        p(1., 1., 1.),
        p(0., 1., 1.),
    ];
    let quads = [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [2, 3, 7, 6],
        [1, 2, 6, 5],
        [0, 4, 7, 3],
    ];
    let mut t = vec![];
    for q in quads {
        t.push([v[q[0]], v[q[1]], v[q[2]]]);
        t.push([v[q[0]], v[q[2]], v[q[3]]]);
    }
    Mesh { triangles: t }
}

/// UV sphere of radius r with outward normals.
pub fn uv_sphere(r: f64, stacks: usize, slices: usize) -> Mesh {
    use std::f64::consts::PI;
    let pt = |i: usize, j: usize| {
        let th = PI * i as f64 / stacks as f64;
        let ph = 2.0 * PI * (j % slices) as f64 / slices as f64;
        [
            r * th.sin() * ph.cos(),
            r * th.sin() * ph.sin(),
            r * th.cos(),
        ]
    };
    let mut t = vec![];
    for i in 0..stacks {
        for j in 0..slices {
            let (a, b, c, d) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
            if i != 0 {
                t.push([a, b, d]);
            }
            if i != stacks - 1 {
                t.push([b, c, d]);
            }
        }
    }
    Mesh { triangles: t }
}

/// Analyse an STL file whose coordinates are in `length_unit`.
pub fn analyse(path: impl AsRef<Path>, length_unit: &str, density_kg_m3: f64) -> Result<Analysis> {
    let (mut asset, bytes) = MediaAsset::load(path)?;
    if asset.kind != MediaKind::Geometry {
        return Err(SimorphError::InvalidInput(format!(
            "{} is not an STL mesh ({})",
            asset.path.display(),
            asset.media_type
        )));
    }
    let to_m = simorph_units::convert(1.0, length_unit, "m")?;
    let mesh = Mesh::parse_stl(&bytes)?.scaled(to_m);
    let topo = mesh.topology();
    let mp = mesh.mass_properties(density_kg_m3);
    asset
        .metadata
        .insert("triangles".into(), mesh.triangles.len().into());
    let mut q = vec![
        Quantity::new("volume", mp.volume.abs(), "m^3"),
        Quantity::new("surface_area", mp.surface_area, "m^2"),
        Quantity::new("mass", mp.mass.abs(), "kg"),
        Quantity::new("centroid_x", mp.centroid[0], "m"),
        Quantity::new("centroid_y", mp.centroid[1], "m"),
        Quantity::new("centroid_z", mp.centroid[2], "m"),
        Quantity::new("inertia_xx", mp.inertia[0][0].abs(), "kg*m^2"),
        Quantity::new("inertia_yy", mp.inertia[1][1].abs(), "kg*m^2"),
        Quantity::new("inertia_zz", mp.inertia[2][2].abs(), "kg*m^2"),
        Quantity::new("watertight", if topo.watertight { 1.0 } else { 0.0 }, "1")
            .with_uncertainty(Uncertainty::Exact),
    ];
    if !topo.watertight {
        for x in q.iter_mut().filter(|x| {
            ["volume", "mass"].contains(&x.name.as_str())
                || x.name.starts_with("inertia")
                || x.name.starts_with("centroid")
        }) {
            x.description =
                Some("mesh is not watertight: solid properties are not meaningful".into());
        }
    }
    Ok(Analysis {
        asset,
        quantities: q,
        details: serde_json::json!({"topology": topo, "mass_properties": mp, "normals_outward": mp.volume > 0.0}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_cube_box_inertia_and_round_trips() {
        let cube = box_mesh(1.0, 1.0, 1.0);
        let mp = cube.mass_properties(1.0);
        assert!((mp.volume - 1.0).abs() < 1e-12);
        assert!((mp.surface_area - 6.0).abs() < 1e-12);
        for k in 0..3 {
            assert!((mp.centroid[k] - 0.5).abs() < 1e-12);
        }
        let (a, b, c, rho) = (2.0, 3.0, 0.5, 7800.0);
        let bx = box_mesh(a, b, c).mass_properties(rho);
        let m = a * b * c * rho;
        assert!((bx.inertia[0][0] - m / 12.0 * (b * b + c * c)).abs() < 1e-8 * m);
        assert!((bx.inertia[1][1] - m / 12.0 * (a * a + c * c)).abs() < 1e-8 * m);
        assert!((bx.inertia[2][2] - m / 12.0 * (a * a + b * b)).abs() < 1e-8 * m);
        assert!(bx.inertia[0][1].abs() < 1e-8 * m);
        let t = cube.topology();
        assert!(t.watertight, "{t:?}");
        assert_eq!(t.euler_characteristic, 2);
        // binary and ascii round trips
        let back = Mesh::parse_stl(&cube.to_binary_stl()).unwrap();
        assert!((back.mass_properties(1.0).volume - 1.0).abs() < 1e-6);
        let back = Mesh::parse_stl(cube.to_ascii_stl("cube").as_bytes()).unwrap();
        assert!((back.mass_properties(1.0).volume - 1.0).abs() < 1e-12);
    }

    #[test]
    fn sphere_volume_within_one_percent_and_holes_detected() {
        let s = uv_sphere(0.3, 64, 128);
        let mp = s.mass_properties(1.0);
        let exact = 4.0 / 3.0 * std::f64::consts::PI * 0.3f64.powi(3);
        eprintln!(
            "MEASURE sphere 64x128: volume rel. error {:.3e}",
            (mp.volume - exact) / exact
        );
        assert!(
            ((mp.volume - exact) / exact).abs() < 0.01,
            "{} vs {exact}",
            mp.volume
        );
        let ia = 0.4 * mp.volume * 0.09;
        assert!(((mp.inertia[2][2] - ia) / ia).abs() < 0.02);
        assert!(s.topology().watertight);
        let mut open = box_mesh(1.0, 1.0, 1.0);
        open.triangles.pop();
        let t = open.topology();
        assert!(!t.watertight);
        assert_eq!(t.boundary_edges, 3);
        let mut flipped = box_mesh(1.0, 1.0, 1.0);
        flipped.triangles[0].swap(1, 2);
        assert!(flipped.topology().orientation_conflicts > 0);
    }

    #[test]
    fn analyse_with_millimetre_units() {
        let d = std::env::temp_dir().join(format!("simorph-mm-geo-{}", simorph_core::RunId::new()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("plate.stl");
        std::fs::write(&p, box_mesh(100.0, 50.0, 10.0).to_binary_stl()).unwrap();
        let a = analyse(&p, "mm", 2700.0).unwrap();
        let v = a.quantity("volume").unwrap();
        assert!((v.value - 5e-5).abs() < 1e-12, "{}", v.value);
        assert!((a.quantity("mass").unwrap().value - 0.135).abs() < 1e-9);
        assert_eq!(a.quantity("watertight").unwrap().value, 1.0);
        assert!(analyse(&p, "furlong", 1.0).is_err());
    }
}
