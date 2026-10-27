//! # Training service (ARC-10, §5.1 "génération de données" and "entraînement")
//!
//! * [`DatasetSpec`] – a dataset either generated synthetically by running a
//!   component over a design (JSON-lines file + SHA-256 + provenance) or
//!   supplied as a file whose hash is verified before use.
//! * [`TrainerSpec`] – in-process `Training` component, sandboxed Python
//!   script (through the Python bridge), or external command.
//! * [`TrainingService::run`] – executes a [`TrainingJob`] and returns the new
//!   candidate, metrics, artifacts (with hashes) and provenance ids.
//! * [`templates`] – ready-to-run Python training script skeletons
//!   (numpy logistic regression; PyTorch/TRL/PEFT SFT and LoRA) that speak the
//!   SIMORPH protocol.  Torch-based templates detect a missing `torch` and
//!   report it instead of pretending to train.

pub mod dataset;
pub mod service;
pub mod templates;

pub use dataset::*;
pub use service::*;
pub use templates::*;
