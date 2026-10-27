//! Worker loop serving a `ComponentSet`.

use crate::auth;
use crate::inventory::ResourceInventory;
use crate::protocol::*;
use crate::wire;
use parking_lot::Mutex;
use simorph_core::*;
use simorph_plugin_sdk::protocol::Response;
use simorph_plugin_sdk::ComponentSet;
use std::collections::HashMap;
use std::io::BufReader;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Prefix of error messages reporting a deadline expiry on the worker.
pub const TIMEOUT_PREFIX: &str = "timeout:";

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub id: String,
    pub secret: Option<Vec<u8>>,
    /// Concurrent tasks accepted (defaults to detected CPU cores).
    pub slots: Option<usize>,
    pub heartbeat_interval: Duration,
}

impl WorkerConfig {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.into(),
            secret: None,
            slots: None,
            heartbeat_interval: Duration::from_millis(500),
        }
    }
}

/// How the worker loop ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerExit {
    Shutdown,
    Disconnected,
    Rejected(String),
}

/// Connect to a coordinator and serve tasks until shutdown or disconnection.
pub fn run_worker(addr: &str, cfg: WorkerConfig, set: ComponentSet) -> Result<WorkerExit> {
    let stream = TcpStream::connect(addr)?;
    let _ = stream.set_nodelay(true);
    let mut reader = BufReader::new(stream.try_clone()?);
    let writer = Arc::new(Mutex::new(stream));
    let Some(CoordinatorMsg::Challenge { nonce, protocol }) = wire::recv(&mut reader)? else {
        return Err(SimorphError::Other(
            "coordinator did not send a challenge".into(),
        ));
    };
    if protocol != DISTRIBUTED_PROTOCOL_VERSION {
        return Err(SimorphError::Incompatible(format!(
            "coordinator protocol `{protocol}`"
        )));
    }
    let inventory = ResourceInventory::detect();
    let slots = cfg.slots.unwrap_or(inventory.cpu_cores).max(1);
    let auth = cfg
        .secret
        .as_ref()
        .map(|s| auth::respond(s, &nonce, &cfg.id));
    wire::send(
        &mut writer.lock(),
        &WorkerMsg::Hello {
            worker_id: cfg.id.clone(),
            protocol: DISTRIBUTED_PROTOCOL_VERSION.into(),
            inventory,
            components: set.names(),
            slots,
            auth,
        },
    )?;
    match wire::recv(&mut reader)? {
        Some(CoordinatorMsg::Welcome { .. }) => {}
        Some(CoordinatorMsg::Rejected { reason }) => return Ok(WorkerExit::Rejected(reason)),
        other => {
            return Err(SimorphError::Other(format!(
                "unexpected handshake reply {other:?}"
            )))
        }
    }
    let running: Arc<Mutex<HashMap<TaskId, CancellationToken>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let stop = Arc::new(AtomicBool::new(false));
    {
        let (w, r, s, every) = (
            writer.clone(),
            running.clone(),
            stop.clone(),
            cfg.heartbeat_interval,
        );
        std::thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                let ids: Vec<TaskId> = r.lock().keys().copied().collect();
                if wire::send(&mut w.lock(), &WorkerMsg::Heartbeat { running: ids }).is_err() {
                    break;
                }
                std::thread::sleep(every);
            }
        });
    }
    let exit = loop {
        match wire::recv::<CoordinatorMsg>(&mut reader) {
            Ok(Some(CoordinatorMsg::Assign {
                task_id,
                component,
                request,
                timeout_ms,
            })) => {
                let token = CancellationToken::new();
                running.lock().insert(task_id, token.clone());
                let (w, r, set) = (writer.clone(), running.clone(), set.clone());
                std::thread::spawn(move || {
                    let ctl = ExecutionControl {
                        cancel: token,
                        deadline: Some(Deadline::after(Duration::from_millis(timeout_ms))),
                    };
                    let response = match set.get(&component) {
                        None => Response::Error { message: format!("component `{component}` not served by this worker"), run_id: Some(request.run_id) },
                        Some(c) => match run_component(c.as_ref(), &request, &ctl) {
                            Ok(result) => Response::Result { result },
                            // Deadline expiry is an infrastructure outcome (retryable by the
                            // coordinator), distinct from a deterministic component error.
                            Err(SimorphError::Timeout(_)) => Response::Error { message: format!("{TIMEOUT_PREFIX} deadline of {timeout_ms} ms exceeded on worker"), run_id: Some(request.run_id) },
                            Err(e) => Response::Error { message: e.to_string(), run_id: Some(request.run_id) },
                        },
                    };
                    let cancelled = ctl.cancel.is_cancelled();
                    r.lock().remove(&task_id);
                    if !cancelled {
                        let _ = wire::send(&mut w.lock(), &WorkerMsg::Done { task_id, response });
                    }
                });
            }
            Ok(Some(CoordinatorMsg::Cancel { task_id })) => {
                if let Some(t) = running.lock().get(&task_id) {
                    t.cancel();
                }
            }
            Ok(Some(CoordinatorMsg::Shutdown)) => break WorkerExit::Shutdown,
            Ok(Some(other)) => tracing::warn!("unexpected message {other:?}"),
            Ok(None) | Err(_) => break WorkerExit::Disconnected,
        }
    };
    stop.store(true, Ordering::SeqCst);
    for t in running.lock().values() {
        t.cancel();
    }
    Ok(exit)
}
