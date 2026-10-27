//! # simorph-distributed
//!
//! Distributed execution of SIMORPH components over TCP (subsystem 16).
//!
//! * [`protocol`] – JSON-lines messages between coordinator and workers.  Task
//!   payloads reuse the component protocol types of `simorph-plugin-sdk`
//!   (`TaskRequest` in, `protocol::Response` out).
//! * [`inventory`] – detection of the resources a worker really has (CPU
//!   cores, memory, GPUs via `nvidia-smi` / `rocm-smi`).  Nothing is invented:
//!   a machine without a detected GPU reports zero GPUs.
//! * [`auth`] – optional shared-secret HMAC-SHA256 challenge/response.
//! * [`coordinator`] – priority queue, resource-aware placement, per-task
//!   timeout, retry on another worker when a worker dies (connection loss or
//!   heartbeat loss), exactly-once result recording, de-duplication by
//!   request hash, cancellation, back-pressure and metrics.
//! * [`worker`] – the worker loop serving a `ComponentSet`; the
//!   `simorph-worker` binary wraps it.
//!
//! ## Mapping onto Ray and Kubernetes (not implemented here)
//!
//! The coordinator interface (`submit` / `wait` / `cancel` / `metrics`) is
//! deliberately the same shape as a Ray task API or a Kubernetes Job queue:
//!
//! * **Ray**: a `simorph-worker` process maps to a Ray actor holding a
//!   `ComponentSet`; `TaskSpec::requirements` maps to Ray resource requests
//!   (`num_cpus`, `num_gpus`, `memory`); retries map to `max_retries`.  An
//!   adapter would submit through Ray's Python API via the python bridge.
//! * **Kubernetes**: each worker is a Pod of a Deployment (or a Job per
//!   batch); `requirements` map to container `resources.requests/limits`
//!   (`nvidia.com/gpu` for GPUs); the coordinator becomes a Service the Pods
//!   connect to, with the HMAC secret mounted from a Secret.
//!
//! Neither adapter exists yet; only the native TCP implementation below is
//! implemented and tested.

pub mod auth;
pub mod coordinator;
pub mod inventory;
pub mod protocol;
pub mod wire;
pub mod worker;

pub use coordinator::*;
pub use inventory::*;
pub use protocol::*;
pub use worker::*;
