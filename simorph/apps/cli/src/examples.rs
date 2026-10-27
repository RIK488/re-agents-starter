//! Example problem specifications (used by `simorph problem example` and by
//! the integration tests).  They exercise different sectors to show that no
//! domain is hard-coded.

use simorph_core::*;

pub fn names() -> Vec<&'static str> {
    vec!["nanosat", "beam", "pipe", "finance"]
}

pub fn example(name: &str) -> Option<ProblemSpec> {
    Some(match name {
        "nanosat" => nanosat(),
        "beam" => beam(),
        "pipe" => pipe(),
        "finance" => finance(),
        _ => return None,
    })
}

/// Nanosatellite power subsystem sizing: maximise the minimum state of
/// charge over one orbit with a mass cap on the solar array.
pub fn nanosat() -> ProblemSpec {
    let mut p = ProblemSpec::new(
        "Dimensionnement énergétique d'un nanosatellite",
        "Choisir la surface de panneau solaire et la capacité batterie pour garantir le bilan d'énergie d'un nanosatellite 3U en orbite basse, sous contrainte de masse.",
    );
    p.domains = vec!["aerospace".into(), "power".into(), "thermal".into()];
    p.knowns
        .push(Quantity::new("altitude", 500.0, "km").with_uncertainty(Uncertainty::Exact));
    p.knowns.push(
        Quantity::new("array_efficiency", 0.28, "1")
            .with_uncertainty(Uncertainty::Normal { std_dev: 0.02 }),
    );
    p.knowns.push(
        Quantity::new("load_power", 14.0, "W").with_uncertainty(Uncertainty::Uniform {
            low: 12.0,
            high: 16.0,
        }),
    );
    p.knowns.push(Quantity::new("soc0", 0.3, "1"));
    p.knowns.push(Quantity::new("orbits", 1.0, "1"));
    p.design_variables.push(DesignVariable {
        name: "array_area".into(),
        unit: "cm^2".into(),
        low: 300.0,
        high: 1200.0,
        integer: false,
        initial: None,
    });
    p.design_variables.push(DesignVariable {
        name: "battery_capacity_ah".into(),
        unit: "Ah".into(),
        low: 2.0,
        high: 10.0,
        integer: false,
        initial: None,
    });
    p.objectives.push(Objective {
        name: "état de charge final maximal".into(),
        direction: Direction::Maximize,
        variable: "final_soc".into(),
        unit: "1".into(),
        target: None,
        weight: 1.0,
    });
    p.constraints.push(Constraint {
        name: "masse panneau".into(),
        variable: "array_mass".into(),
        kind: ConstraintKind::UpperBound,
        bound: 0.35,
        unit: "kg".into(),
        tolerance: 0.0,
        hard: true,
    });
    p.constraints.push(Constraint {
        name: "bilan d'énergie fermé".into(),
        variable: "energy_balance_error".into(),
        kind: ConstraintKind::UpperBound,
        bound: 0.05,
        unit: "1".into(),
        tolerance: 0.0,
        hard: true,
    });
    p.unknowns.push(Unknown {
        name: "eclipse_fraction".into(),
        unit: "1".into(),
        prior: None,
        description: Some("fraction d'orbite en éclipse".into()),
    });
    p.unknowns.push(Unknown {
        name: "min_soc".into(),
        unit: "1".into(),
        prior: None,
        description: Some("état de charge minimal sur l'orbite".into()),
    });
    p.budget.max_runs = 400;
    p
}

pub fn beam() -> ProblemSpec {
    let mut p = ProblemSpec::new("Poutre console", "Minimiser la flèche d'une poutre console en acier sous charge ponctuelle, flèche admissible 20 mm.");
    p.domains = vec!["civil-engineering".into()];
    p.knowns.push(Quantity::new("load", 1.0, "kN"));
    p.knowns.push(Quantity::new("youngs_modulus", 210.0, "GPa"));
    p.design_variables.push(DesignVariable {
        name: "length".into(),
        unit: "mm".into(),
        low: 1000.0,
        high: 3000.0,
        integer: false,
        initial: None,
    });
    p.design_variables.push(DesignVariable {
        name: "second_moment".into(),
        unit: "cm^4".into(),
        low: 500.0,
        high: 2000.0,
        integer: false,
        initial: None,
    });
    p.objectives.push(Objective {
        name: "flèche minimale".into(),
        direction: Direction::Minimize,
        variable: "tip_deflection".into(),
        unit: "m".into(),
        target: None,
        weight: 1.0,
    });
    p.constraints.push(Constraint {
        name: "flèche admissible".into(),
        variable: "tip_deflection".into(),
        kind: ConstraintKind::UpperBound,
        bound: 0.02,
        unit: "m".into(),
        tolerance: 0.0,
        hard: true,
    });
    p.budget.max_runs = 1000;
    p
}

pub fn pipe() -> ProblemSpec {
    let mut p = ProblemSpec::new(
        "Perte de charge d'une conduite minière",
        "Choisir le diamètre d'une conduite d'eau de 2 km pour limiter la perte de charge à 5 bar.",
    );
    p.domains = vec!["mining".into(), "water".into()];
    p.knowns.push(Quantity::new("length", 2.0, "km"));
    p.knowns.push(Quantity::new("velocity", 2.5, "m/s"));
    p.knowns.push(Quantity::new("density", 1000.0, "kg/m^3"));
    p.knowns.push(Quantity::new("viscosity", 1.0, "mPa*s"));
    p.knowns.push(Quantity::new("roughness", 0.05, "mm"));
    p.design_variables.push(DesignVariable {
        name: "diameter".into(),
        unit: "mm".into(),
        low: 100.0,
        high: 600.0,
        integer: false,
        initial: None,
    });
    p.objectives.push(Objective {
        name: "perte de charge minimale".into(),
        direction: Direction::Minimize,
        variable: "pressure_drop".into(),
        unit: "bar".into(),
        target: None,
        weight: 1.0,
    });
    p.constraints.push(Constraint {
        name: "perte admissible".into(),
        variable: "pressure_drop".into(),
        kind: ConstraintKind::UpperBound,
        bound: 5.0,
        unit: "bar".into(),
        tolerance: 0.0,
        hard: true,
    });
    p.budget.max_runs = 500;
    p
}

pub fn finance() -> ProblemSpec {
    let mut p = ProblemSpec::new(
        "Valeur actuelle nette d'un projet",
        "Évaluer la VAN d'un investissement de 100 avec flux 60, 60 au taux de 10 %.",
    );
    p.domains = vec!["finance".into()];
    p.knowns.push(Quantity::new("discount_rate", 0.1, "1"));
    p.unknowns.push(Unknown {
        name: "npv".into(),
        unit: "1".into(),
        prior: None,
        description: None,
    });
    p.unknowns.push(Unknown {
        name: "irr".into(),
        unit: "1".into(),
        prior: None,
        description: None,
    });
    p
}
