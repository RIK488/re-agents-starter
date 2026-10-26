//! Example Rust component served over stdio.  Used by the integration test
//! `rust_subprocess.rs` and as a template for isolating Rust components in a
//! child process.

use simorph_core::{Capability, Port};
use simorph_plugin_sdk::{serve_stdio, ComponentBuilder};

fn main() {
    let component = ComponentBuilder::new("rust.example.affine", "1.0.0", "simorph")
        .capability(Capability::AlgebraicSolver)
        .domain("test")
        .input(Port::scalar("x", "m"))
        .input(Port::scalar("a", "1"))
        .input(Port::scalar("b", "m"))
        .output(Port::scalar("y", "m"))
        .build(|req, ctl| {
            ctl.check()?;
            let y = req.get_f64("a")? * req.get_f64("x")? + req.get_f64("b")?;
            let mut out = indexmap::IndexMap::new();
            out.insert("y".to_string(), serde_json::json!(y));
            Ok(out)
        })
        .expect("valid manifest");
    if let Err(e) = serve_stdio(&component) {
        eprintln!("simorph-example-component: {e}");
        std::process::exit(1);
    }
}
