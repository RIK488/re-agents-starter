//! Lexicons (French and English).  Entries are written in natural form and
//! stemmed at load time with [`crate::text::stem`].

use crate::text::stem;
use std::collections::HashSet;

/// Function words ignored when matching concepts.
pub const STOPWORDS: &[&str] = &[
    "le",
    "la",
    "les",
    "l",
    "un",
    "une",
    "des",
    "du",
    "de",
    "d",
    "au",
    "aux",
    "a",
    "à",
    "en",
    "et",
    "ou",
    "par",
    "pour",
    "sur",
    "avec",
    "sans",
    "dans",
    "est",
    "sont",
    "vaut",
    "valent",
    "egal",
    "égale",
    "égal",
    "environ",
    "ce",
    "cet",
    "cette",
    "ces",
    "son",
    "sa",
    "ses",
    "leur",
    "leurs",
    "qui",
    "que",
    "dont",
    "doit",
    "doivent",
    "peut",
    "être",
    "etre",
    "été",
    "se",
    "s",
    "y",
    "il",
    "elle",
    "ils",
    "on",
    "nous",
    "vous",
    "fixé",
    "fixée",
    "fixe",
    "donné",
    "donnée",
    "également",
    "egalement",
    "ainsi",
    "aussi",
    "soumis",
    "soumise",
    "the",
    "of",
    "an",
    "and",
    "or",
    "to",
    "for",
    "with",
    "in",
    "on",
    "at",
    "is",
    "are",
    "be",
    "equal",
    "equals",
    "about",
    "approximately",
    "its",
    "it",
    "this",
    "that",
    "which",
    "must",
    "should",
    "has",
    "have",
    "also",
    "under",
    "subject",
    "given",
    "set",
    "vaut",
    "reste",
    "rester",
    "comprise",
    "compris",
    "comprises",
    "nominal",
    "nominale",
    "valeur",
    "value",
    "ici",
    "here",
    "chaque",
    "each",
    "tout",
    "toute",
    "all",
    "possible",
    "afin",
    "so",
    "as",
    "than",
    "que",
    "qu",
    "lors",
    "pendant",
    "during",
    "over",
    "sur",
    "couvre",
    "covers",
    "considérer",
    "considerer",
    "consider",
    "on",
    "souhaite",
    "want",
    "we",
    "nous",
    "voulons",
    "veut",
    "wants",
    "objectif",
    "objective",
    "goal",
    "but",
    "problème",
    "probleme",
    "problem",
    "système",
    "systeme",
    "system",
    "étude",
    "etude",
    "study",
    "mesuré",
    "mesurée",
    "measured",
    "typique",
    "typical",
    "vaut",
    "fait",
    "font",
    "makes",
    "en",
    "bout",
    "its",
    "their",
    "his",
    "her",
    "well",
    "bien",
    "très",
    "tres",
    "very",
    "elle",
    "même",
    "meme",
    "same",
    "est-à-dire",
    "i.e",
    "soit",
    "respectivement",
    "respectively",
    "then",
    "puis",
    "ensuite",
];

/// Verbs marking ranges as design variables.
pub const DESIGN_VERBS: &[&str] = &[
    "choisir",
    "choisi",
    "choisie",
    "choix",
    "dimensionner",
    "dimensionnement",
    "ajuster",
    "régler",
    "regler",
    "sélectionner",
    "selectionner",
    "concevoir",
    "choose",
    "select",
    "size",
    "sizing",
    "tune",
    "design",
    "pick",
    "adjust",
    "explore",
    "explorer",
];

/// Words marking a range as an uncertainty.
pub const UNCERTAINTY_WORDS: &[&str] = &[
    "incertain",
    "incertaine",
    "incertains",
    "incertaines",
    "incertitude",
    "aléatoire",
    "aleatoire",
    "varie",
    "fluctue",
    "dispersion",
    "uncertain",
    "uncertainty",
    "varies",
    "fluctuates",
    "random",
    "tolérance",
    "tolerance",
];

/// Words marking soft constraints.
pub const SOFT_WORDS: &[&str] = &[
    "préférence",
    "preference",
    "idéalement",
    "idealement",
    "preferably",
    "ideally",
    "souhaitable",
];

/// Counted nouns that denote an evaluation budget.
pub const BUDGET_NOUNS: &[&str] = &[
    "simulation",
    "simulations",
    "run",
    "runs",
    "évaluation",
    "évaluations",
    "evaluation",
    "evaluations",
    "essai",
    "essais",
];
pub const BUDGET_WORDS: &[&str] = &[
    "budget", "maximum", "plafond", "limité", "limite", "limited", "limit",
];

/// Objective verbs: (stemmed prefix, direction is minimise?).
pub const MIN_VERBS: &[&str] = &[
    "minimiser",
    "minimise",
    "minimiser",
    "minimize",
    "minimisation",
    "minimization",
    "réduire",
    "reduire",
    "reduce",
    "diminuer",
];
pub const MAX_VERBS: &[&str] = &[
    "maximiser",
    "maximise",
    "maximize",
    "maximisation",
    "maximization",
    "augmenter",
    "increase",
];
/// Superlatives implying an objective on a hidden concept: (phrase, minimise?, implied concept words).
pub const SUPERLATIVES: &[(&[&str], bool, &[&str])] = &[
    (&["le", "plus", "léger"], true, &["masse"]),
    (&["la", "plus", "légère"], true, &["masse"]),
    (&["lightest"], true, &["mass"]),
    (&["le", "moins", "cher"], true, &["coût"]),
    (&["cheapest"], true, &["cost"]),
];

/// Unknown markers.
pub const UNKNOWN_VERBS: &[&str] = &[
    "déterminer",
    "determiner",
    "calculer",
    "estimer",
    "prédire",
    "predire",
    "évaluer",
    "evaluer",
    "determine",
    "compute",
    "calculate",
    "estimate",
    "predict",
    "evaluate",
    "quel",
    "quelle",
    "quels",
    "quelles",
    "what",
];

/// Constraint keywords: (word sequence, kind).  `U` upper, `L` lower, `E` equality.
pub const CONSTRAINT_KEYWORDS: &[(&[&str], char)] = &[
    (&["ne", "doit", "pas", "dépasser"], 'U'),
    (&["ne", "doivent", "pas", "dépasser"], 'U'),
    (&["ne", "pas", "dépasser"], 'U'),
    (&["dépasser"], 'U'),
    (&["au", "plus"], 'U'),
    (&["au", "maximum"], 'U'),
    (&["inférieur"], 'U'),
    (&["inférieure"], 'U'),
    (&["inferieur"], 'U'),
    (&["moins", "de"], 'U'),
    (&["pas", "plus", "de"], 'U'),
    (&["maximal"], 'U'),
    (&["maximale"], 'U'),
    (&["≤"], 'U'),
    (&["<="], 'U'),
    (&["<"], 'U'),
    (&["at", "most"], 'U'),
    (&["not", "exceed"], 'U'),
    (&["exceed"], 'U'),
    (&["no", "more", "than"], 'U'),
    (&["less", "than"], 'U'),
    (&["below"], 'U'),
    (&["au", "moins"], 'L'),
    (&["supérieur"], 'L'),
    (&["supérieure"], 'L'),
    (&["superieur"], 'L'),
    (&["minimal"], 'L'),
    (&["minimale"], 'L'),
    (&["≥"], 'L'),
    (&[">="], 'L'),
    (&[">"], 'L'),
    (&["at", "least"], 'L'),
    (&["no", "less", "than"], 'L'),
    (&["greater", "than"], 'L'),
    (&["above"], 'L'),
    (&["positif"], 'L'),
    (&["positive"], 'L'),
    (&["égal", "à"], 'E'),
    (&["égale", "à"], 'E'),
    (&["equal", "to"], 'E'),
];

/// Domain tags triggered by keywords.
pub const DOMAINS: &[(&str, &[&str])] = &[
    (
        "aerospace",
        &[
            "nanosatellite",
            "satellite",
            "orbite",
            "orbit",
            "spatial",
            "spacecraft",
            "cubesat",
            "orbital",
        ],
    ),
    (
        "power",
        &[
            "panneau solaire",
            "solaire",
            "batterie",
            "battery",
            "solar",
            "énergétique",
            "energy",
            "énergie",
        ],
    ),
    (
        "thermal",
        &["thermique", "thermal", "température", "temperature"],
    ),
    (
        "civil-engineering",
        &["poutre", "beam", "console", "cantilever", "génie civil"],
    ),
    ("mining", &["mine", "minière", "minier", "mining"]),
    (
        "water",
        &[
            "eau",
            "water",
            "hydraulique",
            "hydraulic",
            "conduite",
            "pipe",
        ],
    ),
    (
        "finance",
        &[
            "finance",
            "investissement",
            "investment",
            "actualisation",
            "cash",
        ],
    ),
    (
        "operations-research",
        &["file d'attente", "queue", "logistique", "logistics"],
    ),
    ("energy", &["éolienne", "wind", "turbine", "vent"]),
    (
        "biology",
        &["population", "croissance", "growth", "écologie"],
    ),
];

/// Port synonyms (French and English).  Keys are port names of the default
/// component set; ports absent from the catalogue are simply never matched.
pub const SYNONYMS: &[(&str, &[&str])] = &[
    // nanosatellite / power
    (
        "altitude",
        &[
            "altitude",
            "altitude orbite",
            "altitude orbitale",
            "orbital altitude",
            "hauteur orbite",
        ],
    ),
    (
        "array_area",
        &[
            "surface panneau",
            "surface panneau solaire",
            "aire panneau",
            "surface générateur solaire",
            "array area",
            "solar array area",
            "panel area",
        ],
    ),
    (
        "array_efficiency",
        &[
            "rendement panneau",
            "rendement cellule",
            "rendement",
            "efficacité panneau",
            "array efficiency",
            "panel efficiency",
            "cell efficiency",
        ],
    ),
    (
        "load_power",
        &[
            "puissance consommée",
            "puissance plateforme",
            "consommation",
            "consommation électrique",
            "load power",
            "platform power",
            "power consumption",
            "consumed power",
        ],
    ),
    (
        "battery_capacity_ah",
        &["capacité batterie", "battery capacity", "capacité"],
    ),
    (
        "capacity_ah",
        &["capacité batterie", "battery capacity", "capacité"],
    ),
    (
        "soc0",
        &[
            "état charge initial",
            "soc initial",
            "initial state charge",
            "initial soc",
        ],
    ),
    (
        "orbits",
        &["orbite", "nombre orbite", "orbit", "number orbit"],
    ),
    (
        "min_soc",
        &[
            "état charge minimal",
            "soc minimal",
            "minimum state charge",
            "minimum soc",
            "état charge minimum",
        ],
    ),
    (
        "final_soc",
        &[
            "état charge final",
            "soc final",
            "final state charge",
            "final soc",
        ],
    ),
    (
        "eclipse_fraction",
        &[
            "fraction éclipse",
            "eclipse fraction",
            "fraction orbite éclipse",
            "proportion éclipse",
        ],
    ),
    (
        "mean_solar_power",
        &[
            "puissance solaire moyenne",
            "mean solar power",
            "average solar power",
        ],
    ),
    (
        "energy_balance_error",
        &[
            "erreur bilan énergie",
            "erreur bilan énergétique",
            "energy balance error",
            "fermeture bilan énergie",
        ],
    ),
    (
        "energy_margin_wh",
        &[
            "marge énergie",
            "marge énergétique",
            "energy margin",
            "marge bilan énergie",
        ],
    ),
    (
        "depth_of_discharge",
        &[
            "profondeur décharge",
            "depth discharge",
            "décharge maximale",
            "dod",
        ],
    ),
    (
        "max_array_temperature",
        &[
            "température maximale panneau",
            "maximum array temperature",
            "température max panneau",
        ],
    ),
    (
        "min_array_temperature",
        &[
            "température minimale panneau",
            "minimum array temperature",
            "température min panneau",
        ],
    ),
    (
        "curtailed_energy",
        &["énergie écrêtée", "énergie perdue", "curtailed energy"],
    ),
    (
        "power_system_mass",
        &[
            "masse sous-système énergie",
            "masse système énergie",
            "masse sous-système électrique",
            "power system mass",
            "power subsystem mass",
        ],
    ),
    (
        "energy_balance_abs_error",
        &[
            "erreur fermeture bilan numérique",
            "fermeture bilan numérique",
            "erreur bilan numérique",
            "numerical balance error",
        ],
    ),
    (
        "array_mass",
        &[
            "masse panneau",
            "masse générateur solaire",
            "array mass",
            "panel mass",
            "masse panneau solaire",
            "masse",
        ],
    ),
    (
        "orbital_period",
        &["période orbitale", "orbital period", "période orbite"],
    ),
    // structures
    (
        "length",
        &[
            "longueur",
            "long",
            "length",
            "portée",
            "span",
            "longueur conduite",
            "pipe length",
        ],
    ),
    (
        "load",
        &[
            "charge ponctuelle",
            "charge",
            "effort",
            "force",
            "load",
            "tip load",
            "point load",
        ],
    ),
    (
        "youngs_modulus",
        &[
            "module young",
            "module élasticité",
            "young modulus",
            "elastic modulus",
            "module",
        ],
    ),
    (
        "second_moment",
        &[
            "moment quadratique",
            "moment inertie",
            "inertie section",
            "second moment area",
            "second moment",
            "area moment inertia",
        ],
    ),
    (
        "tip_deflection",
        &[
            "flèche",
            "flèche bout",
            "déflexion",
            "deflection",
            "tip deflection",
            "déplacement bout",
        ],
    ),
    (
        "max_bending_moment",
        &[
            "moment fléchissant",
            "moment fléchissant maximal",
            "bending moment",
            "maximum bending moment",
        ],
    ),
    (
        "tip_slope",
        &["rotation bout", "pente bout", "tip slope", "tip rotation"],
    ),
    // hydraulics
    (
        "diameter",
        &["diamètre", "diameter", "diamètre conduite", "pipe diameter"],
    ),
    (
        "velocity",
        &[
            "vitesse",
            "vitesse écoulement",
            "velocity",
            "flow velocity",
            "flow speed",
        ],
    ),
    ("density", &["masse volumique", "densité", "density"]),
    (
        "viscosity",
        &[
            "viscosité",
            "viscosité dynamique",
            "viscosity",
            "dynamic viscosity",
        ],
    ),
    ("roughness", &["rugosité", "roughness"]),
    (
        "pressure_drop",
        &[
            "perte charge",
            "chute pression",
            "pressure drop",
            "head loss",
            "pressure loss",
        ],
    ),
    (
        "reynolds",
        &["nombre reynolds", "reynolds", "reynolds number"],
    ),
    (
        "friction_factor",
        &["coefficient frottement", "friction factor"],
    ),
    // finance / operations / energy / biology
    ("discount_rate", &["taux actualisation", "discount rate"]),
    (
        "npv",
        &["valeur actuelle nette", "van", "npv", "net present value"],
    ),
    (
        "irr",
        &[
            "taux rendement interne",
            "tri",
            "irr",
            "internal rate return",
        ],
    ),
    ("payback_period", &["délai récupération", "payback period"]),
    (
        "arrival_rate",
        &["taux arrivée", "arrival rate", "débit arrivée"],
    ),
    ("service_rate", &["taux service", "service rate"]),
    (
        "utilization",
        &["taux occupation", "utilisation", "utilization"],
    ),
    (
        "mean_wait",
        &[
            "temps attente moyen",
            "attente moyenne",
            "mean waiting time",
            "mean wait",
        ],
    ),
    ("mean_sojourn", &["temps séjour", "mean sojourn time"]),
    (
        "mean_customers",
        &["nombre moyen client", "mean number customers"],
    ),
    ("wind_speed", &["vitesse vent", "wind speed"]),
    ("rotor_diameter", &["diamètre rotor", "rotor diameter"]),
    (
        "air_density",
        &["masse volumique air", "densité air", "air density"],
    ),
    (
        "power_coefficient",
        &["coefficient puissance", "power coefficient"],
    ),
    ("rated_power", &["puissance nominale", "rated power"]),
    ("cut_in", &["vitesse démarrage", "cut in speed"]),
    ("cut_out", &["vitesse coupure", "cut out speed"]),
    (
        "initial_population",
        &["population initiale", "initial population"],
    ),
    (
        "carrying_capacity",
        &["capacité accueil", "carrying capacity"],
    ),
    ("growth_rate", &["taux croissance", "growth rate"]),
    ("population", &["population", "population finale"]),
    (
        "t_end",
        &[
            "durée simulation",
            "horizon simulation",
            "simulation duration",
            "temps final",
            "horizon",
        ],
    ),
    (
        "T_env",
        &[
            "température ambiante",
            "température environnement",
            "ambient temperature",
        ],
    ),
    ("irradiance", &["irradiance", "éclairement", "flux solaire"]),
];

pub struct Lexicon {
    pub stop: HashSet<String>,
    pub known_words: HashSet<String>,
    pub synonyms: Vec<(String, Vec<Vec<String>>)>,
}

/// Stem every word of a phrase, dropping stopwords.
pub fn content_tokens(phrase: &str, stop: &HashSet<String>) -> Vec<String> {
    phrase
        .split(|c: char| c.is_whitespace() || c == '\'' || c == '’' || c == '-')
        .map(stem)
        .filter(|w| !w.is_empty() && !stop.contains(w))
        .collect()
}

impl Lexicon {
    pub fn load() -> Self {
        let stop: HashSet<String> = STOPWORDS.iter().map(|w| stem(w)).collect();
        let mut known: HashSet<String> = stop.clone();
        let mut add = |ws: &[&str]| {
            for w in ws {
                for p in w.split_whitespace() {
                    known.insert(stem(p));
                }
            }
        };
        add(DESIGN_VERBS);
        add(UNCERTAINTY_WORDS);
        add(SOFT_WORDS);
        add(BUDGET_NOUNS);
        add(BUDGET_WORDS);
        add(MIN_VERBS);
        add(MAX_VERBS);
        add(UNKNOWN_VERBS);
        for (k, _) in CONSTRAINT_KEYWORDS {
            add(k);
        }
        for (_, ws) in DOMAINS {
            add(ws);
        }
        let mut synonyms = vec![];
        for (port, phrases) in SYNONYMS {
            add(phrases);
            let toks: Vec<Vec<String>> = phrases
                .iter()
                .map(|p| content_tokens(p, &stop))
                .filter(|t| !t.is_empty())
                .collect();
            synonyms.push((port.to_string(), toks));
        }
        Self {
            stop,
            known_words: known,
            synonyms,
        }
    }

    pub fn is_stop(&self, norm: &str) -> bool {
        self.stop.contains(norm)
    }

    pub fn synonyms_of(&self, port: &str) -> Vec<Vec<String>> {
        let mut out: Vec<Vec<String>> = self
            .synonyms
            .iter()
            .filter(|(p, _)| p == port)
            .flat_map(|(_, s)| s.clone())
            .collect();
        // automatic English synonym from the port name itself: wind_speed → [wind, speed]
        let auto: Vec<String> = port
            .split('_')
            .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_alphabetic()))
            .map(stem)
            .filter(|t| !self.stop.contains(t))
            .collect();
        if !auto.is_empty() {
            out.push(auto);
        }
        out
    }
}

pub fn contains_word(words: &[&str], norm: &str) -> bool {
    words.iter().any(|w| stem(w) == norm)
}
