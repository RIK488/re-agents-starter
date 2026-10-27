//! Audio analyses (WAV via `hound`, spectrum via `rustfft`).

use crate::asset::{Analysis, MediaAsset, MediaKind};
use rustfft::{num_complex::Complex, FftPlanner};
use simorph_core::{Quantity, Result, SimorphError, Uncertainty};
use std::path::Path;

/// Mono samples in [-1, 1] and the sample rate.
pub fn read_wav(path: impl AsRef<Path>) -> Result<(Vec<f64>, u32, u16)> {
    let mut r = hound::WavReader::open(path.as_ref())
        .map_err(|e| SimorphError::InvalidInput(format!("wav: {e}")))?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let raw: Vec<f64> = match spec.sample_format {
        hound::SampleFormat::Float => r
            .samples::<f32>()
            .map(|s| s.map(|v| v as f64))
            .collect::<std::result::Result<_, _>>(),
        hound::SampleFormat::Int => {
            let scale = (1u64 << (spec.bits_per_sample - 1)) as f64;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f64 / scale))
                .collect::<std::result::Result<_, _>>()
        }
    }
    .map_err(|e| SimorphError::InvalidInput(format!("wav samples: {e}")))?;
    let mono = raw
        .chunks(ch)
        .map(|c| c.iter().sum::<f64>() / ch as f64)
        .collect();
    Ok((mono, spec.sample_rate, spec.channels))
}

/// Dominant frequency (Hz) by Hann-windowed FFT with parabolic interpolation
/// of the log-magnitude peak.  Uses at most `max_len` samples.
pub fn dominant_frequency(samples: &[f64], sample_rate: u32, max_len: usize) -> Option<(f64, f64)> {
    let n_avail = samples.len().min(max_len);
    if n_avail < 16 {
        return None;
    }
    let n = n_avail.next_power_of_two();
    let mut buf: Vec<Complex<f64>> = (0..n)
        .map(|i| {
            if i < n_avail {
                let w = 0.5
                    - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (n_avail - 1) as f64).cos();
                Complex::new(samples[i] * w, 0.0)
            } else {
                Complex::new(0.0, 0.0)
            }
        })
        .collect();
    FftPlanner::new().plan_fft_forward(n).process(&mut buf);
    let mag: Vec<f64> = buf[..n / 2].iter().map(|c| c.norm()).collect();
    let (k, _) = mag
        .iter()
        .enumerate()
        .skip(1)
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())?;
    let bin = sample_rate as f64 / n as f64;
    let delta = if k > 0 && k + 1 < mag.len() {
        let (a, b, c) = (
            mag[k - 1].max(1e-300).ln(),
            mag[k].max(1e-300).ln(),
            mag[k + 1].max(1e-300).ln(),
        );
        let den = a - 2.0 * b + c;
        if den.abs() > 1e-300 {
            0.5 * (a - c) / den
        } else {
            0.0
        }
    } else {
        0.0
    };
    Some(((k as f64 + delta) * bin, bin))
}

pub fn analyse(path: impl AsRef<Path>) -> Result<Analysis> {
    let (mut asset, _) = MediaAsset::load(path.as_ref())?;
    if asset.kind != MediaKind::Audio {
        return Err(SimorphError::InvalidInput(format!(
            "{} is not a WAV file ({})",
            asset.path.display(),
            asset.media_type
        )));
    }
    let (s, sr, ch) = read_wav(path)?;
    let duration = s.len() as f64 / sr as f64;
    let rms = (s.iter().map(|v| v * v).sum::<f64>() / s.len().max(1) as f64).sqrt();
    let peak = s.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    asset.metadata.insert("sample_rate".into(), sr.into());
    asset.metadata.insert("channels".into(), ch.into());
    let mut q = vec![
        Quantity::new("duration", duration, "s").with_uncertainty(Uncertainty::Interval {
            low: duration - 0.5 / sr as f64,
            high: duration + 0.5 / sr as f64,
        }),
        Quantity::new("sample_rate", sr as f64, "Hz").with_uncertainty(Uncertainty::Exact),
        Quantity::dimensionless("rms", rms),
        Quantity::dimensionless("peak", peak),
    ];
    if let Some((f, bin)) = dominant_frequency(&s, sr, 1 << 18) {
        q.push(
            Quantity::new("dominant_frequency", f, "Hz").with_uncertainty(Uncertainty::Interval {
                low: f - bin / 2.0,
                high: f + bin / 2.0,
            }),
        );
    }
    Ok(Analysis {
        asset,
        quantities: q,
        details: serde_json::json!({"samples": s.len()}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn write_tone(path: &Path, f: f64, amp: f64, sr: u32, secs: f64, stereo: bool) {
        let spec = hound::WavSpec {
            channels: if stereo { 2 } else { 1 },
            sample_rate: sr,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(sr as f64 * secs) as usize {
            let v = (amp * (2.0 * std::f64::consts::PI * f * i as f64 / sr as f64).sin() * 32767.0)
                as i16;
            w.write_sample(v).unwrap();
            if stereo {
                w.write_sample(v).unwrap();
            }
        }
        w.finalize().unwrap();
    }

    #[test]
    fn tone_440_hz() {
        let d =
            std::env::temp_dir().join(format!("simorph-mm-audio-{}", simorph_core::RunId::new()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("a440.wav");
        write_tone(&p, 440.0, 0.5, 44_100, 1.0, false);
        let a = analyse(&p).unwrap();
        let f = a.quantity("dominant_frequency").unwrap().value;
        eprintln!(
            "MEASURE 440 Hz tone: estimate {f:.4} Hz (error {:.2e} Hz)",
            f - 440.0
        );
        assert!((f - 440.0).abs() < 1.0, "{f}");
        assert!((a.quantity("rms").unwrap().value - 0.5 / 2f64.sqrt()).abs() < 1e-3);
        assert!((a.quantity("duration").unwrap().value - 1.0).abs() < 1e-9);
        let p2 = d.join("stereo.wav");
        write_tone(&p2, 1234.5, 0.3, 48_000, 0.5, true);
        let b = analyse(&p2).unwrap();
        assert!((b.quantity("dominant_frequency").unwrap().value - 1234.5).abs() < 1.0);
        assert_eq!(b.asset.metadata["channels"], 2);
    }
}
