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

/// Nanosatellite power subsystem sizing: minimise the mass of the power
/// subsystem (array + battery) such that the battery gains energy over an
/// orbit (energy-positive) and its depth of discharge stays ≤ 30 % (cycle
/// life).  This formulation is not saturated: a larger array raises the
/// margin but costs mass, a larger battery lowers the depth of discharge but
/// costs mass.
pub fn nanosat() -> ProblemSpec {
    let mut p = ProblemSpec::new(
        "Dimensionnement énergétique d'un nanosatellite",
        "Choisir la surface de panneau solaire et la capacité batterie qui minimisent la masse du sous-système énergie d'un nanosatellite en orbite basse à 500 km, avec un bilan d'énergie positif sur l'orbite et une profondeur de décharge d'au plus 30 %.",
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
    p.knowns.push(Quantity::new("soc0", 0.6, "1"));
    p.knowns.push(Quantity::new("orbits", 2.0, "1"));
    p.design_variables.push(DesignVariable {
        name: "array_area".into(),
        unit: "cm^2".into(),
        low: 300.0,
        high: 1500.0,
        integer: false,
        initial: None,
    });
    p.design_variables.push(DesignVariable {
        name: "battery_capacity_ah".into(),
        unit: "Ah".into(),
        low: 1.0,
        high: 12.0,
        integer: false,
        initial: None,
    });
    p.objectives.push(Objective {
        name: "masse minimale du sous-système énergie".into(),
        direction: Direction::Minimize,
        variable: "power_system_mass".into(),
        unit: "kg".into(),
        target: None,
        weight: 1.0,
    });
    p.constraints.push(Constraint {
        name: "bilan d'énergie positif".into(),
        variable: "energy_margin_wh".into(),
        kind: ConstraintKind::LowerBound,
        bound: 0.0,
        unit: "Wh".into(),
        tolerance: 0.0,
        hard: true,
    });
    p.constraints.push(Constraint {
        name: "profondeur de décharge".into(),
        variable: "depth_of_discharge".into(),
        kind: ConstraintKind::UpperBound,
        bound: 0.3,
        unit: "1".into(),
        tolerance: 0.0,
        hard: true,
    });
    p.constraints.push(Constraint {
        name: "fermeture du bilan numérique".into(),
        variable: "energy_balance_abs_error".into(),
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
        description: Some("état de charge minimal".into()),
    });
    p.unknowns.push(Unknown {
        name: "max_array_temperature".into(),
        unit: "K".into(),
        prior: None,
        description: Some("température maximale du panneau".into()),
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
