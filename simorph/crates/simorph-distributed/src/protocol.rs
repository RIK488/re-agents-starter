//! Coordinator ⇄ worker messages (JSON-lines).

use crate::inventory::ResourceInventory;
use serde::{Deserialize, Serialize};
use simorph_core::{TaskId, TaskRequest};
use simorph_plugin_sdk::protocol::Response;

pub const DISTRIBUTED_PROTOCOL_VERSION: &str = "simorph-distributed/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoordinatorMsg {
    Challenge {
        nonce: String,
        protocol: String,
    },
    Welcome {
        worker_id: String,
    },
    Rejected {
        reason: String,
    },
    Assign {
        task_id: TaskId,
        component: String,
        request: TaskRequest,
        timeout_ms: u64,
    },
    Cancel {
        task_id: TaskId,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerMsg {
    Hello {
        worker_id: String,
        protocol: String,
        inventory: ResourceInventory,
        components: Vec<String>,
        slots: usize,
        auth: Option<String>,
    },
    Heartbeat {
        running: Vec<TaskId>,
    },
    /// Terminal response of an assigned task, using the component protocol type.
    Done {
        task_id: TaskId,
        response: Response,
    },
}
