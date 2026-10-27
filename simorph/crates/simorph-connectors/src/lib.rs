//! # simorph-connectors
//!
//! Connections between SIMORPH and real data and instruments (subsystem 19).
//!
//! * [`source`] – the [`source::DataSource`] trait: a schema whose numeric
//!   columns carry validated units, pagination, and a provenance record
//!   (URI, SHA-256 of the bytes read, retrieval time).  Implementations:
//!   [`csv_source::CsvSource`] (header units `temperature [degC]`),
//!   [`jsonl::JsonlSource`], [`http::HttpJsonSource`].  Unknown units are
//!   rejected when the schema is built, never guessed.
//! * [`calibration`] – turning source rows into calibration observations
//!   (inputs, output, value, standard deviation) in the units a model expects.
//! * [`instrument`] – the [`instrument::Instrument`] trait.  Reads are free;
//!   setpoints require `Permission::ExternalSideEffect` through
//!   `simorph_security::Policy` (an autonomous agent needs a human approval).
//!   [`instrument::ReplayInstrument`] replays a recorded CSV and labels every
//!   reading as replayed; [`instrument::LineInstrument`] speaks a SCPI-like
//!   text protocol over any `Read + Write` transport (serial device file,
//!   TCP socket, test pipe).

pub mod calibration;
pub mod csv_source;
pub mod http;
pub mod instrument;
pub mod jsonl;
pub mod source;

pub use calibration::*;
pub use csv_source::CsvSource;
pub use http::{HttpJsonSource, Pagination};
pub use instrument::*;
pub use jsonl::JsonlSource;
pub use source::*;
