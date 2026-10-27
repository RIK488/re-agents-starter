//! Markdown rendering for the "Espace Modélisation".

use crate::assembly::ModelAssembly;
use simorph_model_registry::ModelDescriptor;

pub fn descriptor_markdown(d: &ModelDescriptor) -> String {
    let mut s = format!("### {} v{} — {:?}\n\n", d.name, d.version, d.formalism);
    if let Some(desc) = &d.description {
        s.push_str(&format!("{desc}\n\n"));
    }
    if !d.equations.is_empty() {
        s.push_str("**Équations**\n\n");
        for e in &d.equations {
            s.push_str(&format!("- `{} = {}`\n", e.name, e.expression));
        }
        s.push('\n');
    }
    s.push_str("| Variable | Rôle | Unité |\n|---|---|---|\n");
    for p in &d.states {
        s.push_str(&format!("| {} | état | {} |\n", p.name, p.unit));
    }
    for p in &d.inputs {
        s.push_str(&format!("| {} | entrée | {} |\n", p.name, p.unit));
    }
    for p in &d.outputs {
        s.push_str(&format!("| {} | sortie | {} |\n", p.name, p.unit));
    }
    if !d.parameters.is_empty() {
        s.push_str("\n**Paramètres** : ");
        s.push_str(
            &d.parameters
                .iter()
                .map(|q| format!("{} = {} {}", q.name, q.value, q.unit))
                .collect::<Vec<_>>()
                .join(", "),
        );
        s.push('\n');
    }
    if !d.assumptions.is_empty() {
        s.push_str("\n**Hypothèses**\n");
        for a in &d.assumptions {
            s.push_str(&format!("- {a}\n"));
        }
    }
    if !d.validity_domain.ranges.is_empty() {
        s.push_str("\n**Domaine de validité** : ");
        s.push_str(
            &d.validity_domain
                .ranges
                .iter()
                .map(|(k, (lo, hi))| format!("{k} ∈ [{lo}, {hi}]"))
                .collect::<Vec<_>>()
                .join(", "),
        );
        s.push('\n');
    }
    if !d.dependencies.is_empty() {
        s.push_str(&format!(
            "\n**Dépendances** : {}\n",
            d.dependencies
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !d.verification_criteria.is_empty() {
        s.push_str("\n**Critères de vérification**\n");
        for c in &d.verification_criteria {
            s.push_str(&format!("- {} : `{}`\n", c.name, c.expression));
        }
    }
    s.push_str(&format!("\nIncertitude : {:?}\n", d.uncertainty));
    s
}

pub fn assembly_markdown(a: &ModelAssembly) -> String {
    let mut s = String::from("## Assemblage de modèles\n\n");
    s.push_str(&format!(
        "Chaîne : {}\n\n",
        a.chain
            .steps
            .iter()
            .map(|st| st.name.clone())
            .collect::<Vec<_>>()
            .join(" → ")
    ));
    if !a.unresolved.is_empty() {
        s.push_str(&format!(
            "**Grandeurs non productibles** : {}\n\n",
            a.unresolved.join(", ")
        ));
    }
    s.push_str("### Connexions\n\n| Producteur | Port | Consommateur | Port | Facteur | Décalage |\n|---|---|---|---|---|---|\n");
    for c in &a.connections {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {:.6e} | {:.6e} |\n",
            c.producer, c.out_port, c.consumer, c.in_port, c.conversion_factor, c.conversion_offset
        ));
    }
    s.push_str(
        "\n### Contrôles dimensionnels\n\n| Contrôle | Résultat | Détails |\n|---|---|---|\n",
    );
    for c in &a.dimensional_report {
        s.push_str(&format!(
            "| {} | {} | {} |\n",
            c.name,
            if c.passed { "✓" } else { "✗" },
            c.details.replace('|', "\\|")
        ));
    }
    s.push_str("\n### Modèles\n\n");
    for d in &a.descriptors {
        s.push_str(&descriptor_markdown(d));
        s.push('\n');
    }
    s.push_str("### Modèle composite\n\n");
    s.push_str(&descriptor_markdown(&a.composite));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use simorph_core::{Port, Quantity};

    #[test]
    fn renders_equations_and_variables() {
        let d = crate::from_equations(
            "decay",
            &["d(x)/dt = -k*x"],
            &[Port::scalar("x", "mol")],
            &[Quantity::new("k", 0.5, "1/s")],
        )
        .unwrap();
        let md = descriptor_markdown(&d);
        assert!(md.contains("`d(x)/dt = -k*x`"));
        assert!(md.contains("| x | état | mol |"));
        assert!(md.contains("x_0"));
    }
}
