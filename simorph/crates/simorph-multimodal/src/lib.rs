//! # simorph-multimodal
//!
//! Typed media assets and analyses producing SIMORPH [`Quantity`] values with
//! units (subsystem 7).
//!
//! | Module | Media | Analyses |
//! |---|---|---|
//! | [`image`] | PNG, JPEG, PPM | size, channel statistics, histograms, Sobel edges, connected components, pixel-scale calibration to m² |
//! | [`audio`] | WAV (PCM int / float) | duration, RMS, peak, dominant frequency (Hann-windowed FFT + parabolic interpolation) |
//! | [`video`] | anything ffmpeg reads | probe (duration, fps, resolution, frame count) and frame extraction, via the `ffmpeg`/`ffprobe` CLI |
//! | [`geometry`] | STL ASCII / binary | parse/write, volume, area, centroid, inertia tensor (Eberly), watertightness/manifold check |
//! | [`text`] | UTF-8 text | NFKC normalisation, chunking, extraction of numeric quantities with unit validation |
//!
//! [`components`] exposes every analysis as a `simorph_core::Component`.
//!
//! [`Quantity`]: simorph_core::Quantity

pub mod asset;
pub mod audio;
pub mod components;
pub mod geometry;
pub mod image;
pub mod text;
pub mod video;

pub use asset::*;
