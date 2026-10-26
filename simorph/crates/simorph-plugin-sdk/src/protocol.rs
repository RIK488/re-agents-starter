//! SIMORPH JSON-lines component protocol.
//!
//! One JSON object per line, UTF-8, `\n` terminated, on stdin (requests)
//! and stdout (responses).  The child must never write anything else to
//! stdout; diagnostics go to stderr.  See `adapters/PROTOCOL.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use simorph_core::{ComponentManifest, RunId, TaskRequest, TaskResult};

/// Messages sent by SIMORPH to the component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Ask for the manifest.
    Manifest,
    /// Execute a task.
    Execute { request: TaskRequest },
    /// Cancel the running task (best effort).
    Cancel,
    /// Terminate the server.
    Shutdown,
}

/// Messages sent by the component to SIMORPH.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Manifest {
        manifest: Box<ComponentManifest>,
    },
    Result {
        result: TaskResult,
    },
    Error {
        message: String,
        #[serde(default)]
        run_id: Option<RunId>,
    },
    /// Progress heartbeat; `fraction` in [0,1].
    Progress {
        fraction: f64,
        #[serde(default)]
        message: Option<String>,
    },
    /// Acknowledgement of `Cancel` / `Shutdown`.
    Ack,
}

/// Protocol version advertised in `adapters/PROTOCOL.md`.
pub const PROTOCOL_VERSION: &str = "1.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_is_tagged() {
        let s = serde_json::to_string(&Request::Manifest).unwrap();
        assert_eq!(s, r#"{"type":"manifest"}"#);
        let p: Response = serde_json::from_str(r#"{"type":"progress","fraction":0.5}"#).unwrap();
        assert!(matches!(p, Response::Progress { fraction, .. } if fraction == 0.5));
    }
}
