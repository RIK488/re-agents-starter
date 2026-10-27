//! # Simulator factory (ARC-04, §3.4)
//!
//! [`SimulatorFactory::run`] executes the ten mandatory steps and returns a
//! [`GenerationReport`] with one [`StepReport`] per step, each recorded in
//! the provenance ledger:
//!
//! 1. search – candidates from the registry by required outputs and domain,
//!    or user-supplied equations when nothing is registered;
//! 2. compatibility – units/shapes of inputs and outputs, validity-domain
//!    coverage of the known values;
//! 3. configure – solver choice from a stiffness estimate (Jacobian norm ×
//!    horizon, explicit trial run) and fidelity → tolerances;
//! 4. integrate – wrap as `OdeComponent` and register into the
//!    [`ComponentSet`];
//! 5. complete – derive missing outputs from declared algebraic relations
//!    (dimensionally checked);
//! 6. generate – build a component from equations (in-process) or emit a
//!    Python component script run through the sandboxed Python bridge; the
//!    script text and SHA-256 go to provenance; generated components are at
//!    most `Simulated` and carry `provenance.generated_by`;
//! 7. test – reference cases and invariants through `simorph-validation`;
//!    a generated simulator without references is explicitly "non validé";
//! 8. compare – Euler/RK4/RK45/BackwardEuler/Trapezoidal against the
//!    tightest RK45 reference: error vs wall time vs rhs evaluations;
//! 9. improve – cheapest method meeting the target error becomes the default;
//! 10. document – limits written into the manifest, Markdown documentation.
//!
//! Nothing here fabricates a result: a required output that cannot be
//! produced ends with step 6 `Failed` and a precise reason.

mod factory;
mod python;
mod report;
mod wrapper;

pub use factory::*;
pub use python::generate_python_script;
pub use report::*;
pub use wrapper::FactoryComponent;
