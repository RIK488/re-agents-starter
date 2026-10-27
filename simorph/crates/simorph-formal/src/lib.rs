//! # simorph-formal — mathematical and formal verification (subsystem 13)
//!
//! | Module | Capability | Backend |
//! |---|---|---|
//! | [`expr`], [`formula`] | AST for real/integer/boolean formulas, SMT-LIB 2 printer, native evaluation, dimension inference | native |
//! | [`smt`], [`prover`] | prove `premises ⇒ goal` or decide feasibility; certificates with SHA-256, fresh re-run, independent second opinions | Z3 (sandboxed Python helper) |
//! | [`interval`] | outward-rounded interval arithmetic, branch-and-bound bound certification | native |
//! | [`linear`] | LP second opinion for linear arithmetic | SciPy / HiGHS |
//! | [`enumerate`] | exhaustive decision for small finite domains | native |
//! | [`symbolic`] | ODE-solution, equivalence and Jacobian checks | SymPy + native cross-check |
//! | [`lean`] | Lean 4 file checking | `lean` binary when present |
//! | [`integration`] | proof artifacts, claim promotion, ProblemSpec feasibility | simorph-validation |
//!
//! Evidence policy: `MathematicallyProven` is reachable only through an SMT
//! certificate that is decisive, reproducible in a fresh process, confirmed
//! by an independent method, and intact (hash).  SymPy results are never
//! promoted to proofs.

pub mod enumerate;
pub mod expr;
pub mod formula;
pub mod integration;
pub mod interval;
pub mod lean;
pub mod linear;
pub mod prover;
pub mod runner;
pub mod smt;
pub mod symbolic;

pub use expr::{Expr, Func};
pub use formula::{Cmp, Formula, Problem, Sort};
pub use integration::{
    certificate_checks, judge_claim, prove_claim, spec_feasibility, to_proof_artifact,
    SpecFeasibility,
};
pub use interval::{check_atom_on_box, enclose, BoundCheck, Enclosure, Interval};
pub use lean::LeanBackend;
pub use prover::{
    check_feasibility, prove, recheck, Agreement, Certificate, Mode, ProveOptions, Recheck, Verdict,
};
pub use runner::{run_tool, solver_policy, tool_versions};
pub use symbolic::{
    jacobian, verify_equivalence, verify_ode_solution, JacobianCheck, SymbolicCertificate,
};
