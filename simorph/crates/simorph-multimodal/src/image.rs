//! Image analyses (PNG/JPEG/PPM via the `image` crate).

use crate::asset::{Analysis, MediaAsset, MediaKind};
use serde::{Deserialize, Serialize};
use simorph_core::{Quantity, Result, SimorphError, Uncertainty};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelStats {
    pub mean: f64,
    pub std: f64,
    pub min: u8,
    pub max: u8,
    pub histogram: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blob {
    pub area_px: usize,
    /// (x_min, y_min, x_max, y_max), inclusive.
    pub bbox: (u32, u32, u32, u32),
    pub centroid_px: (f64, f64),
}

/// Decoded 8-bit RGB image.
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<[u8; 3]>,
}

impl Raster {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let img = image::load_from_memory(bytes)
            .map_err(|e| SimorphError::InvalidInput(format!("image decode: {e}")))?;
        let rgb = img.to_rgb8();
        let (width, height) = rgb.dimensions();
        Ok(Self {
            width,
            height,
            rgb: rgb.pixels().map(|p| p.0).collect(),
        })
    }
    /// ITU-R BT.601 luma.
    pub fn luma(&self) -> Vec<f64> {
        self.rgb
            .iter()
            .map(|p| 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64)
            .collect()
    }
    pub fn channel_stats(&self) -> [ChannelStats; 3] {
        let n = self.rgb.len().max(1) as f64;
        let mk = |c: usize| {
            let mut hist = vec![0u32; 256];
            let (mut s, mut s2, mut mn, mut mx) = (0.0, 0.0, 255u8, 0u8);
            for p in &self.rgb {
                let v = p[c];
                hist[v as usize] += 1;
                s += v as f64;
                s2 += (v as f64).powi(2);
                mn = mn.min(v);
                mx = mx.max(v);
            }
            let mean = s / n;
            ChannelStats {
                mean,
                std: (s2 / n - mean * mean).max(0.0).sqrt(),
                min: mn,
                max: mx,
                histogram: hist,
            }
        };
        [mk(0), mk(1), mk(2)]
    }
    /// Sobel gradient magnitude of the luma image (borders set to 0).
    pub fn sobel(&self) -> Vec<f64> {
        let (w, h) = (self.width as usize, self.height as usize);
        let l = self.luma();
        let mut g = vec![0.0; w * h];
        for y in 1..h.saturating_sub(1) {
            for x in 1..w.saturating_sub(1) {
                let p = |dx: isize, dy: isize| {
                    l[(y as isize + dy) as usize * w + (x as isize + dx) as usize]
                };
                let gx =
                    -p(-1, -1) - 2.0 * p(-1, 0) - p(-1, 1) + p(1, -1) + 2.0 * p(1, 0) + p(1, 1);
                let gy =
                    -p(-1, -1) - 2.0 * p(0, -1) - p(1, -1) + p(-1, 1) + 2.0 * p(0, 1) + p(1, 1);
                g[y * w + x] = (gx * gx + gy * gy).sqrt();
            }
        }
        g
    }
    /// 8-connected components of pixels whose luma is above (or below, for
    /// dark objects) `threshold`.
    pub fn components(&self, threshold: f64, dark_objects: bool) -> Vec<Blob> {
        let (w, h) = (self.width as usize, self.height as usize);
        let l = self.luma();
        let fg: Vec<bool> = l
            .iter()
            .map(|v| {
                if dark_objects {
                    *v < threshold
                } else {
                    *v > threshold
                }
            })
            .collect();
        let mut label = vec![false; w * h];
        let mut blobs = vec![];
        let mut stack = vec![];
        for start in 0..w * h {
            if !fg[start] || label[start] {
                continue;
            }
            label[start] = true;
            stack.push(start);
            let (mut area, mut sx, mut sy) = (0usize, 0.0, 0.0);
            let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
            while let Some(i) = stack.pop() {
                let (x, y) = (i % w, i / w);
                area += 1;
                sx += x as f64;
                sy += y as f64;
                x0 = x0.min(x as u32);
                y0 = y0.min(y as u32);
                x1 = x1.max(x as u32);
                y1 = y1.max(y as u32);
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let j = ny as usize * w + nx as usize;
                        if fg[j] && !label[j] {
                            label[j] = true;
                            stack.push(j);
                        }
                    }
                }
            }
            blobs.push(Blob {
                area_px: area,
                bbox: (x0, y0, x1, y1),
                centroid_px: (sx / area as f64, sy / area as f64),
            });
        }
        blobs.sort_by_key(|b| std::cmp::Reverse(b.area_px));
        blobs
    }
}

/// Options of [`analyse`].
#[derive(Debug, Clone)]
pub struct ImageOptions {
    pub threshold: f64,
    pub dark_objects: bool,
    /// Physical size of one pixel edge (m/px); enables areas in m².
    pub pixel_scale_m: Option<f64>,
    /// Gradient magnitude above which a pixel counts as an edge.
    pub edge_threshold: f64,
}

impl Default for ImageOptions {
    fn default() -> Self {
        Self {
            threshold: 128.0,
            dark_objects: false,
            pixel_scale_m: None,
            edge_threshold: 100.0,
        }
    }
}

pub fn analyse(path: impl AsRef<Path>, opts: &ImageOptions) -> Result<Analysis> {
    let (mut asset, bytes) = MediaAsset::load(path)?;
    if asset.kind != MediaKind::Image {
        return Err(SimorphError::InvalidInput(format!(
            "{} is not an image ({})",
            asset.path.display(),
            asset.media_type
        )));
    }
    let r = Raster::decode(&bytes)?;
    let stats = r.channel_stats();
    let g = r.sobel();
    let edge_fraction =
        g.iter().filter(|v| **v > opts.edge_threshold).count() as f64 / g.len().max(1) as f64;
    let blobs = r.components(opts.threshold, opts.dark_objects);
    asset.metadata.insert("width".into(), r.width.into());
    asset.metadata.insert("height".into(), r.height.into());
    let mut q = vec![
        Quantity::new("width", r.width as f64, "1").with_uncertainty(Uncertainty::Exact),
        Quantity::new("height", r.height as f64, "1").with_uncertainty(Uncertainty::Exact),
        Quantity::dimensionless(
            "mean_luma",
            r.luma().iter().sum::<f64>() / (r.rgb.len().max(1)) as f64,
        ),
        Quantity::dimensionless("edge_fraction", edge_fraction),
        Quantity::new("object_count", blobs.len() as f64, "1").with_uncertainty(Uncertainty::Exact),
    ];
    if let Some(b) = blobs.first() {
        q.push(Quantity::new(
            "largest_object_area_px",
            b.area_px as f64,
            "1",
        ));
        if let Some(s) = opts.pixel_scale_m {
            // Pixel quantisation: ±half a pixel along the perimeter (bbox-based bound).
            let (bw, bh) = (
                (b.bbox.2 - b.bbox.0 + 1) as f64,
                (b.bbox.3 - b.bbox.1 + 1) as f64,
            );
            let half_perimeter_px = bw + bh;
            q.push(
                Quantity::new("largest_object_area", b.area_px as f64 * s * s, "m^2")
                    .with_uncertainty(Uncertainty::Interval {
                        low: (b.area_px as f64 - half_perimeter_px).max(0.0) * s * s,
                        high: (b.area_px as f64 + half_perimeter_px) * s * s,
                    }),
            );
        }
    }
    let details = serde_json::json!({
        "channels": stats.iter().map(|c| serde_json::json!({"mean": c.mean, "std": c.std, "min": c.min, "max": c.max})).collect::<Vec<_>>(),
        "histograms": stats.iter().map(|c| c.histogram.clone()).collect::<Vec<_>>(),
        "objects": blobs,
        "edge_threshold": opts.edge_threshold,
    });
    Ok(Analysis {
        asset,
        quantities: q,
        details,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("simorph-mm-img-{}", simorph_core::RunId::new()));
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    fn scene() -> image::RgbImage {
        let mut img = image::RgbImage::new(200, 100);
        for y in 10..40 {
            for x in 20..40 {
                img.put_pixel(x, y, image::Rgb([255, 255, 255]));
            }
        }
        for y in 60..70 {
            for x in 150..160 {
                img.put_pixel(x, y, image::Rgb([200, 200, 200]));
            }
        }
        img
    }

    #[test]
    fn components_areas_and_calibrated_area() {
        let p = tmp("scene.png");
        scene().save(&p).unwrap();
        let a = analyse(
            &p,
            &ImageOptions {
                pixel_scale_m: Some(0.001),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(a.asset.media_type, "image/png");
        assert_eq!(a.quantity("object_count").unwrap().value, 2.0);
        assert_eq!(a.quantity("largest_object_area_px").unwrap().value, 600.0);
        let area = a.quantity("largest_object_area").unwrap();
        assert!((area.value - 600.0e-6).abs() < 1e-15);
        assert_eq!(area.unit, "m^2");
        let objs = a.details["objects"].as_array().unwrap();
        assert_eq!(objs[1]["area_px"], 100);
        assert_eq!(objs[0]["centroid_px"][0].as_f64().unwrap(), 29.5);
    }

    #[test]
    fn sobel_detects_a_vertical_step_only_at_the_boundary() {
        let mut img = image::RgbImage::new(20, 10);
        for y in 0..10 {
            for x in 10..20 {
                img.put_pixel(x, y, image::Rgb([255, 255, 255]));
            }
        }
        let p = tmp("step.png");
        img.save(&p).unwrap();
        let (_, bytes) = MediaAsset::load(&p).unwrap();
        let r = Raster::decode(&bytes).unwrap();
        let g = r.sobel();
        for y in 1..9 {
            for x in 1..19 {
                let v = g[y * 20 + x];
                if x == 9 || x == 10 {
                    assert!((v - 4.0 * 255.0).abs() < 1e-6, "x={x} v={v}");
                } else {
                    assert_eq!(v, 0.0);
                }
            }
        }
    }

    #[test]
    fn jpeg_and_ppm_decode() {
        let p = tmp("scene.jpg");
        scene().save(&p).unwrap();
        let a = analyse(&p, &ImageOptions::default()).unwrap();
        assert_eq!(a.asset.media_type, "image/jpeg");
        assert_eq!(a.quantity("width").unwrap().value, 200.0);
        let p2 = tmp("scene.ppm");
        scene().save(&p2).unwrap();
        let b = analyse(&p2, &ImageOptions::default()).unwrap();
        assert_eq!(b.quantity("largest_object_area_px").unwrap().value, 600.0);
        assert!(analyse(tmp_text(), &ImageOptions::default()).is_err());
    }

    fn tmp_text() -> std::path::PathBuf {
        let p = tmp("x.txt");
        std::fs::write(&p, "not an image").unwrap();
        p
    }
}
