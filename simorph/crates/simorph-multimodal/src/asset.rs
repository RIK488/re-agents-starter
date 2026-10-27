//! Media assets: kind detection by magic bytes, hashing, metadata.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use simorph_core::{Quantity, Result, SimorphError};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Text,
    Image,
    Audio,
    Video,
    Geometry,
    Table,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaAsset {
    pub kind: MediaKind,
    pub media_type: String,
    pub path: PathBuf,
    pub sha256: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub metadata: IndexMap<String, serde_json::Value>,
}

/// Result of an analysis: physical quantities plus detailed data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub asset: MediaAsset,
    pub quantities: Vec<Quantity>,
    pub details: serde_json::Value,
}

impl Analysis {
    pub fn quantity(&self, name: &str) -> Option<&Quantity> {
        self.quantities.iter().find(|q| q.name == name)
    }
}

/// Detect the media kind of raw bytes (and the file extension as a fallback).
pub fn detect(bytes: &[u8], ext: Option<&str>) -> (MediaKind, String) {
    let ext = ext.map(|e| e.to_ascii_lowercase());
    let starts = |m: &[u8]| bytes.len() >= m.len() && &bytes[..m.len()] == m;
    if starts(b"\x89PNG\r\n\x1a\n") {
        return (MediaKind::Image, "image/png".into());
    }
    if starts(&[0xFF, 0xD8, 0xFF]) {
        return (MediaKind::Image, "image/jpeg".into());
    }
    if bytes.len() > 2
        && bytes[0] == b'P'
        && (b'1'..=b'7').contains(&bytes[1])
        && bytes[2].is_ascii_whitespace()
    {
        return (MediaKind::Image, "image/x-portable-anymap".into());
    }
    if bytes.len() >= 12 && starts(b"RIFF") && &bytes[8..12] == b"WAVE" {
        return (MediaKind::Audio, "audio/wav".into());
    }
    if bytes.len() >= 12 && starts(b"RIFF") && &bytes[8..12] == b"AVI " {
        return (MediaKind::Video, "video/x-msvideo".into());
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return (MediaKind::Video, "video/mp4".into());
    }
    if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return (MediaKind::Video, "video/x-matroska".into());
    }
    match ext.as_deref() {
        Some("stl") => return (MediaKind::Geometry, "model/stl".into()),
        Some("csv") => return (MediaKind::Table, "text/csv".into()),
        Some("tsv") => return (MediaKind::Table, "text/tab-separated-values".into()),
        _ => {}
    }
    if bytes.len() >= 84 {
        let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        if 84 + 50 * n == bytes.len() {
            return (MediaKind::Geometry, "model/stl".into());
        }
    }
    if std::str::from_utf8(bytes).is_ok() {
        let t = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
        if t.trim_start().starts_with("solid") && t.contains("facet") {
            return (MediaKind::Geometry, "model/stl".into());
        }
        return (MediaKind::Text, "text/plain".into());
    }
    (MediaKind::Table, "application/octet-stream".into())
}

impl MediaAsset {
    pub fn load(path: impl AsRef<Path>) -> Result<(Self, Vec<u8>)> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|e| SimorphError::Io(format!("{}: {e}", path.display())))?;
        let (kind, media_type) = detect(&bytes, path.extension().and_then(|e| e.to_str()));
        Ok((
            Self {
                kind,
                media_type,
                path: path.to_path_buf(),
                sha256: simorph_core::hashing::sha256_hex(&bytes),
                size_bytes: bytes.len() as u64,
                metadata: IndexMap::new(),
            },
            bytes,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_bytes() {
        assert_eq!(detect(b"\x89PNG\r\n\x1a\nxxxx", None).0, MediaKind::Image);
        assert_eq!(detect(b"RIFF\0\0\0\0WAVEfmt ", None).0, MediaKind::Audio);
        assert_eq!(detect(b"\0\0\0\x18ftypisom", None).0, MediaKind::Video);
        assert_eq!(
            detect(b"solid x\n facet normal 0 0 1", None).0,
            MediaKind::Geometry
        );
        assert_eq!(detect(b"a,b\n1,2", Some("csv")).0, MediaKind::Table);
        assert_eq!(
            detect("Une charge de 2 kN".as_bytes(), None).0,
            MediaKind::Text
        );
    }
}
