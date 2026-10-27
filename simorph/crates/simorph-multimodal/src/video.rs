//! Video probing and frame extraction through the `ffprobe` / `ffmpeg` CLI.
//!
//! SIMORPH does not decode video itself: ffmpeg is the mature component
//! (LGPL/GPL depending on build — invoked as a separate process, never linked).

use crate::asset::{Analysis, MediaAsset, MediaKind};
use serde::{Deserialize, Serialize};
use simorph_core::{Quantity, Result, SimorphError, Uncertainty};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_s: f64,
    pub frames: Option<u64>,
    pub codec: String,
}

/// Whether ffmpeg and ffprobe are runnable.
pub fn ffmpeg_available() -> std::result::Result<String, String> {
    let out = Command::new("ffprobe")
        .arg("-version")
        .output()
        .map_err(|e| format!("ffprobe not runnable: {e}"))?;
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map_err(|e| format!("ffmpeg not runnable: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .to_string())
}

fn parse_rate(r: &str) -> Option<f64> {
    match r.split_once('/') {
        Some((a, b)) => {
            let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
            if b == 0.0 {
                None
            } else {
                Some(a / b)
            }
        }
        None => r.parse().ok(),
    }
}

pub fn probe(path: impl AsRef<Path>) -> Result<VideoInfo> {
    ffmpeg_available().map_err(SimorphError::NotImplemented)?;
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,nb_frames,codec_name:format=duration",
            "-of",
            "json",
        ])
        .arg(path.as_ref())
        .output()?;
    if !out.status.success() {
        return Err(SimorphError::InvalidInput(format!(
            "ffprobe: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let s = v["streams"]
        .get(0)
        .ok_or_else(|| SimorphError::InvalidInput("no video stream".into()))?;
    Ok(VideoInfo {
        width: s["width"].as_u64().unwrap_or(0) as u32,
        height: s["height"].as_u64().unwrap_or(0) as u32,
        fps: s["r_frame_rate"]
            .as_str()
            .and_then(parse_rate)
            .unwrap_or(f64::NAN),
        duration_s: v["format"]["duration"]
            .as_str()
            .and_then(|d| d.parse().ok())
            .unwrap_or(f64::NAN),
        frames: s["nb_frames"].as_str().and_then(|n| n.parse().ok()),
        codec: s["codec_name"].as_str().unwrap_or("").to_string(),
    })
}

/// Extract frames at `fps` into `out_dir` as PNG; returns the frame paths in order.
pub fn extract_frames(
    path: impl AsRef<Path>,
    out_dir: impl AsRef<Path>,
    fps: f64,
) -> Result<Vec<PathBuf>> {
    ffmpeg_available().map_err(SimorphError::NotImplemented)?;
    let out_dir = out_dir.as_ref();
    std::fs::create_dir_all(out_dir)?;
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(path.as_ref())
        .args(["-vf", &format!("fps={fps}")])
        .arg(out_dir.join("frame_%05d.png"))
        .status()?;
    if !status.success() {
        return Err(SimorphError::Component {
            component: "ffmpeg".into(),
            message: format!("frame extraction failed ({status})"),
        });
    }
    let mut frames: Vec<PathBuf> = std::fs::read_dir(out_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("frame_"))
                .unwrap_or(false)
        })
        .collect();
    frames.sort();
    Ok(frames)
}

pub fn analyse(path: impl AsRef<Path>) -> Result<Analysis> {
    let (mut asset, _) = MediaAsset::load(path.as_ref())?;
    if asset.kind != MediaKind::Video {
        return Err(SimorphError::InvalidInput(format!(
            "{} is not a video ({})",
            asset.path.display(),
            asset.media_type
        )));
    }
    let info = probe(path)?;
    asset
        .metadata
        .insert("codec".into(), info.codec.clone().into());
    let mut q = vec![
        Quantity::new("duration", info.duration_s, "s"),
        Quantity::new("frame_rate", info.fps, "Hz").with_uncertainty(Uncertainty::Exact),
        Quantity::new("width", info.width as f64, "1"),
        Quantity::new("height", info.height as f64, "1"),
    ];
    if let Some(n) = info.frames {
        q.push(Quantity::new("frame_count", n as f64, "1"));
    }
    Ok(Analysis {
        asset,
        quantities: q,
        details: serde_json::to_value(&info)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_and_extract_generated_test_video() {
        if let Err(e) = ffmpeg_available() {
            eprintln!("SKIP: {e}");
            return;
        }
        let d =
            std::env::temp_dir().join(format!("simorph-mm-video-{}", simorph_core::RunId::new()));
        std::fs::create_dir_all(&d).unwrap();
        let v = d.join("test.mp4");
        let ok = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=320x240:rate=10",
                "-c:v",
                "mpeg4",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&v)
            .status()
            .unwrap();
        assert!(ok.success());
        let a = analyse(&v).unwrap();
        assert_eq!(a.quantity("width").unwrap().value, 320.0);
        assert_eq!(a.quantity("height").unwrap().value, 240.0);
        assert!((a.quantity("frame_rate").unwrap().value - 10.0).abs() < 1e-9);
        assert!((a.quantity("duration").unwrap().value - 2.0).abs() < 0.15);
        let frames = extract_frames(&v, d.join("frames"), 5.0).unwrap();
        assert!((9..=11).contains(&frames.len()), "{}", frames.len());
        let img = crate::image::analyse(&frames[0], &Default::default()).unwrap();
        assert_eq!(img.quantity("width").unwrap().value, 320.0);
    }
}
