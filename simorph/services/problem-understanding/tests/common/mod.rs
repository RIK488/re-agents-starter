#![allow(dead_code)]
use simorph_core::*;
use simorph_plugin_sdk::ComponentSet;
use std::sync::Arc;

#[path = "../../../../apps/cli/src/examples.rs"]
pub mod examples;

pub fn manifests() -> Vec<ComponentManifest> {
    let mut v = simorph_plugin_physics::manifests();
    v.extend(simorph_plugin_engineering::manifests());
    v.extend(simorph_plugin_ai::manifests());
    v
}

pub fn component_set() -> ComponentSet {
    let set = ComponentSet::new();
    for c in simorph_plugin_physics::components() {
        set.register_instance(Arc::from(c)).unwrap();
    }
    for m in simorph_plugin_engineering::manifests() {
        set.register_instance(Arc::from(simorph_plugin_engineering::build(m.id).unwrap()))
            .unwrap();
    }
    for m in simorph_plugin_ai::manifests() {
        set.register_instance(Arc::from(simorph_plugin_ai::build(m.id).unwrap()))
            .unwrap();
    }
    set
}

pub const NANOSAT_FR: &str = "Dimensionnement énergétique d'un nanosatellite en orbite basse. Le satellite évolue à une altitude de 500 km. Le rendement du panneau vaut 0,28 ± 0,02. La puissance consommée par la plateforme vaut 14 W, incertaine entre 12 et 16 W. L'état de charge initial de la batterie est de 0,6. La simulation couvre 2 orbites. Choisir la surface du panneau solaire entre 300 et 1500 cm² et la capacité de la batterie entre 1 et 12 Ah pour minimiser la masse du sous-système énergie. La marge d'énergie doit rester supérieure à 0 Wh. La profondeur de décharge ne doit pas dépasser 0,3. L'erreur de fermeture du bilan numérique doit rester inférieure à 0,05. Déterminer également la fraction d'éclipse, l'état de charge minimal et la température maximale du panneau. Budget : 400 simulations.";

pub const BEAM_FR: &str = "Une poutre console en acier est soumise à une charge ponctuelle de 1 kN en bout. Le module d'Young de l'acier vaut 210 GPa. Choisir la longueur entre 1000 et 3000 mm et le moment quadratique de la section entre 500 et 2000 cm⁴ pour minimiser la flèche en bout. La flèche ne doit pas dépasser 20 mm. Budget : 1000 simulations.";

pub const PIPE_FR: &str = "Une conduite d'eau d'une mine mesure 2 km de long. La vitesse d'écoulement est de 2,5 m/s, la masse volumique de 1000 kg/m³, la viscosité dynamique de 1 mPa·s et la rugosité de 0,05 mm. Choisir le diamètre entre 100 et 600 mm pour minimiser la perte de charge, qui ne doit pas dépasser 5 bar. Budget : 500 simulations.";

pub const BEAM_EN: &str = "Size a steel cantilever beam carrying a point load of 1 kN at its tip. Young's modulus is 210 GPa. Choose the length between 1000 and 3000 mm and the second moment of area between 500 and 2000 cm^4 to minimise the tip deflection, which must not exceed 20 mm. Budget: 1000 runs.";
