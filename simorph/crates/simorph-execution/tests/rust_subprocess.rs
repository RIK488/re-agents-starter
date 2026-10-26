//! End-to-end: a Rust component isolated in a child process through
//! `serve_stdio`, driven by `SubprocessComponent`.

use simorph_core::{run_component, Component, ExecutionControl, TaskRequest};
use simorph_execution::SubprocessComponent;
use simorph_security::SandboxPolicy;

#[test]
fn rust_component_served_over_stdio() {
    let bin = env!("CARGO_BIN_EXE_simorph-example-component");
    let c =
        SubprocessComponent::new(bin, vec![], SandboxPolicy::strict().with_timeout(10.0)).unwrap();
    assert_eq!(c.manifest().name, "rust.example.affine");
    assert_eq!(c.manifest().units["y"], "m");
    let req = TaskRequest::new(c.manifest().id)
        .input("x", 2.0)
        .input("a", 3.0)
        .input("b", 0.5)
        .seed(7);
    let r = run_component(&c, &req, &ExecutionControl::default()).unwrap();
    assert_eq!(r.get_f64("y").unwrap(), 6.5);
    assert_eq!(r.request_hash, req.content_hash());
    // missing input is rejected by the parent-side compatibility check
    let bad = TaskRequest::new(c.manifest().id).input("x", 2.0);
    assert!(run_component(&c, &bad, &ExecutionControl::default()).is_err());
    c.shutdown();
}
