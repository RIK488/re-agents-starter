# SIMORPH — Registre d'état (source de vérité)

Version 2, 2026-10-27, après l'audit intégral (`audit/AUDIT-2026-10-27.md`)
et les travaux qui l'ont suivi. Remplace la version 1 (même date), qui
déclarait « implémenté et testé » des éléments dont l'audit a montré les
défauts.

**Règle.** Un sous-système n'est dit *opérationnel* que si un test automatisé
le compare à une référence indépendante (solution analytique, bibliothèque
mature, relance indépendante, témoin négatif), et seulement dans le périmètre
indiqué. Le nombre de tests et la couverture ne sont pas des preuves de
capacité ; ils sont donnés pour situer l'effort de vérification.

Niveaux : **Opérationnel** · **Expérimental** (fonctionne dans un périmètre
restreint, limites listées) · **Absent** · **Recherche** (capacité non
acquise scientifiquement, `RESEARCH_REQUIREMENTS.md`).

## 0. Chiffres du workspace

| Mesure | Valeur |
|---|---|
| Paquets Cargo | 43 (23 crates, 11 services, 3 adaptateurs Rust, 3 plugins, 2 applications, 1 paquet de tests d'intégration) |
| Lignes de Rust | 69 637 |
| Lignes de Python (adaptateurs, moteurs scientifiques, formel) | 1 695 |
| Tests (`#[test]` et `#[tokio::test]`) | 525 déclarés ; résultat du dernier passage complet au §6 |
| Couverture de lignes avant l'audit | 88,1 % (`audit/coverage-2026-10-27.md`) ; non remesurée après les ajouts |
| Environnement | 4 cœurs, 15 Go, pas de GPU, pas de Docker, pas de cluster |

## 1. Les vingt sous-systèmes

| # | Sous-système | Niveau | Ce qui est démontré (test ou mesure) | Limites connues |
|---|---|---|---|---|
| 1 | Noyau Rust | Opérationnel | contrats `Component`, unités, provenance chaînée, ordonnanceur DAG, exécuteur sous-processus ; les campagnes passent par le coordinateur distribué (`campaign run --workers N`) avec résultats identiques à l'exécution locale | l'ordonnanceur et l'orchestrateur de runs restent hors du chemin principal des campagnes locales |
| 2 | Multi-agents | Expérimental | 7 agents typés sur bus supervisé ; permissions par rôle ; `SelfModify` et `ExternalSideEffect` toujours refusés et audités ; validateur indépendant qui relance le composant au point retenu ; rejeu déterministe identique message par message (`services/agents/tests/sessions.rs`, test web de bout en bout) | raisonnement des agents par règles ; seul le critique a un proposeur LLM optionnel, testé contre un serveur factice |
| 3 | Compréhension et décomposition | Expérimental | énoncés FR/EN → `ProblemSpec` ; textes nanosat, poutre, conduite et poutre anglaise équivalents aux spécifications écrites à la main, conclusions identiques à 10⁻⁶ ; extraction de quantités P = R = 1,000 sur 36 phrases étiquetées (0,946 / 0,972 avant deux corrections suscitées par ce même jeu, qui n'est donc pas un jeu de test indépendant) ; ambiguïtés et passages non compris signalés, jamais devinés | lexiques et règles ; pas de jeu de test indépendant ; client LLM non exercé sur un vrai modèle |
| 4 | Génération de modèles | Opérationnel (périmètre borné) | EDO explicites et modèles algébriques à partir d'équations typées ; chaînage dimensionnel ; découverte de lois : Kepler retrouvée à 7,5·10⁻⁵ d'erreur hors échantillon, homogène en dimension, 6 364 évaluations, 1,2 s | pas d'EDP ni de DAE ; Modelica bloqué (OpenModelica inaccessible ici) |
| 5 | Génération de simulateurs | Expérimental | pipeline en 10 étapes : réutilisation, transcription d'équations, génération de code, vérification ; réservoir généré à partir d'équations : 0,036631 m contre 0,036631 m analytique | ne crée pas de physique sans équations ni données (**Recherche RR-1**) ; refus explicite dans ce cas |
| 6 | Cosimulation | Opérationnel (natif) | Jacobi, Gauss-Seidel, itératif avec retour arrière, boucles algébriques, conversion d'unités ; équivalence avec la solution monolithique | FMU : lecture de `modelDescription.xml` seulement (FMPy non installé) ; stabilité non analysée a priori (**RR-7**) |
| 7 | Multimodal | Opérationnel (analyses) | images, signaux audio, géométrie, tableaux : grandeurs à unités, empreintes ; 14 tests | aucun modèle appris de vision ou d'audio (GPU requis) |
| 8 | Calcul scientifique | Opérationnel | natif : RK45 2,19 M pas/s, RK4 5,5 M pas/s, implicites ; SciPy : Robertson à 6·10⁻¹², Van der Pol μ = 1000, LP Wyndor exact ; MuJoCo : pendule θ₀ = 2,5 rad à 2·10⁻⁷ de l'intégrale elliptique ; SymPy | pas d'EDP (FEniCSx, PETSc), pas de Chrono ni de Warp |
| 9 | Expérimentation autonome | Opérationnel (composants existants) | programme à critères figés et hachés ; conclusions limitées aux runs faisables ; inconnues lues à la solution retenue ; calibration sur observations CSV et niveau « validé empiriquement » seulement si z-test et χ² passent ; nanosat : masse 0,4177 kg sous les trois contraintes, 52 runs sur 400, 21 s | « gain informatif » heuristique, non information-théorique ; contradictions signalées mais non relancées (**RR-6**) |
| 10 | Évolution d'algorithmes | Opérationnel | programmation génétique dimensionnelle, MAP-Elites, évolution de programmes par mutations syntaxiques évaluées par instances scellées | pas de mutations par LLM en conditions réelles |
| 11 | Entraînement | Opérationnel (CPU) | MLP natif (Adam) ; PyTorch CPU déterministe (même empreinte de poids sur deux exécutions) | pas de GPU ; pas de fine-tuning de modèles de langage (TRL, PEFT, DeepSpeed) |
| 12 | Renforcement | Opérationnel (petite échelle) | PPO et REINFORCE natifs ; CartPole Gymnasium recoupé avec l'implémentation native à 2,2·10⁻⁸ | pas de RL distribué (RLlib) ni d'environnements lourds |
| 13 | Vérification formelle | Opérationnel (énoncés bornés) | Z3 : preuves et réfutations certifiées, relancées à l'identique et confirmées par une méthode indépendante (intervalles, LP, énumération) ; seul ce chemin accorde « démontré mathématiquement » | Lean 4 absent (renvoie `NotImplemented` avec la cause) ; pas de preuve sur trajectoires flottantes (**RR-3**) |
| 14 | Évaluation indépendante | Expérimental | suites scellées, bootstrap apparié, métriques protégées, témoins négatifs (étiquettes corrompues, simulateur bogué) rejetés | Inspect AI et lm-evaluation-harness non installés ; aucun modèle de langage évalué |
| 15 | Mémoire scientifique | Opérationnel (local) | provenance chaînée vérifiée, mode à clé HMAC ; graphe de connaissances ; archive d'expériences reproductibles par identifiant | pas de PostgreSQL ni de base vectorielle ; transferts interdisciplinaires non évalués automatiquement (**RR-5**) |
| 16 | Distribué | Opérationnel (TCP, un hôte) | coordinateur et workers authentifiés par HMAC, dé-duplication par empreinte de requête, reprise sur panne ; campagne répartie identique à l'exécution locale | pas de GPU, ni Ray, ni Jobs Kubernetes, ni chiffrement du transport ; un seul hôte physique testé |
| 17 | Génération et exécution sûres | Opérationnel | espace de noms réseau vide, `RLIMIT_DATA`, `RLIMIT_CPU`, `RLIMIT_FSIZE`, `RLIMIT_CORE`, groupe de processus tué, environnement nettoyé, plafond de sortie, échec fermé si l'isolation est indisponible (4 tests d'isolation) ; expressions évaluées en Python sur liste blanche | pas de restriction du système de fichiers (montages) ni de limite de processus (cgroups) ; sondes d'attaque dynamiques non exécutées |
| 18 | Interface de laboratoire | Opérationnel | CLI et API HTTP ; chaque action visible correspond à une route réelle (test) ; rôles par clé API, lecture seule pour `Viewer`, CORS fermé par défaut ; panneaux langage naturel, agents, formel, amélioration, découverte | pas d'OIDC ni de multi-utilisateur réel ; pas de visualisation volumique |
| 19 | Connecteurs | Opérationnel (fichiers et protocoles) | CSV et JSON-lines à unités et empreintes, HTTP, rejeu, SCPI sur transport ; écriture vers un instrument soumise à approbation humaine | aucun instrument physique branché ; aucune donnée mesurée réelle |
| 20 | Amélioration continue | **Opérationnel sur réseaux de substitution** | conduite : RMSE log10 0,1132 → 0,0153 (−86,5 %) en 3 itérations acceptées, 168 simulations ; pendule : 0,0125 → 0,0029 (−76,8 %) ; chaque gain a un IC 95 % bootstrap apparié > 0, sans régression des métriques protégées ; étiquettes corrompues et simulateur bogué rejetés ; mesure honnête : l'échantillonnage ciblé n'est pas toujours meilleur que l'uniforme | non démontré sur un modèle de langage ; pas d'auto-modification du code de SIMORPH (**RR-2, RR-8**) |

## 2. Défauts de l'audit et état de correction

### 2.1 Services, applications, plugins

| # | Défaut | État | Test de non-régression |
|---|---|---|---|
| S1, S4 | conclusions sans filtre des contraintes dures ; inconnues lues sur un run arbitraire | corrigé | `campaign-runtime/tests/regressions.rs` |
| S2 | bornes de contraintes non converties pour l'optimiseur | corrigé | `…::constraint_units_reach_optimiser_and_conclusion_is_feasible` |
| S3 | niveaux de preuve recopiés du composant ; calibration inaccessible | corrigé | `…::calibration_grants_empirical_validation_only_when_consistent` |
| S5 | révision d'hypothèses sans mémoire | corrigé | tests du directeur |
| S6 | variable de conception ignorée non signalée | corrigé | tests du directeur |
| S7 | « programme complété » avec 0 run | corrigé (`NothingToRun`) | `…::nothing_to_run_is_reported` |
| S8 | couplage thermique inversé du nanosatellite | corrigé | `solar::energy_tests`, `nanosat::array_runs_cooler_at_higher_efficiency_and_dod_is_consistent` |
| S9 | objectif nanosat saturé et pervers | reformulé (masse sous contraintes) | `tests/integration/tests/acceptance.rs` |
| S10 | améliorations garanties par construction, pseudo-réplications, suite scellée redéfinissable | graines réelles ; redéfinition refusée (`Protected`) ; boucle d'amélioration remplacée par des cibles non triviales | `laboratory::candidate_cannot_redefine_sealed_task_inputs`, `…::replications_are_independent_samples` |
| S11 | web sans RBAC, CORS permissif, tâches non bornées, GET qui écrit | corrigé | `apps/web/tests/routes.rs::viewer_keys_are_read_only_and_cors_is_closed_by_default` |
| S12 | transferts interdisciplinaires jamais produits | ouvert (**RR-5**) | — |
| S13 | évaluation et entraînement non reliés | partiellement : boucle d'amélioration reliée ; services évaluation et entraînement toujours hors chemin | — |
| Nouveau | double conversion d'unités des variables de conception après la correction E6 (flèche de poutre 10× trop faible), détectée par le validateur indépendant des agents | corrigé | `campaign-runtime::tests::beam_campaign_runs_end_to_end_with_unit_conversion`, `agents::beam_from_french_text_end_to_end_and_replay` |

### 2.2 Moteurs (E1–E15)

Le détail des corrections est consigné dans `audit/AUDIT-2026-10-27.md` §9,
avec les tests `tests/audit_regressions.rs` de chaque crate concernée
(validation, simulation, cosimulation, expérimentation, optimisation,
incertitude).

### 2.3 Noyau d'exécution et sécurité

| Contrôle | Avant | Maintenant |
|---|---|---|
| Réseau, mémoire, CPU, taille de fichiers | déclarés, non appliqués | appliqués par le noyau, échec fermé |
| Course du plafond de sortie | test intermittent | contrôle après chaque ligne |
| Signature d'approbation | SHA-256(secret ‖ message) | HMAC-SHA256 (vecteur RFC 4231), comparaison en temps constant |
| Sceaux protégés | empreinte recalculable | sceau à clé disponible (`seal_keyed`) |

## 3. Bibliothèques open source réellement exécutées

SciPy 1.18.1, SymPy 1.14.0, Z3 5.3.2, MuJoCo 3.17.0, Gymnasium 1.4.0,
PyTorch 2.14.1 (CPU), NumPy. Chacune est appelée derrière le contrat
`Component`, en processus isolé, et recoupée avec une référence
indépendante. Les 28 dépôts demandés et leur état : `OSS_INTEGRATION_MATRIX.md`.

## 4. Ce qui n'est pas démontré

* Amélioration d'un modèle de langage : aucun modèle, aucun GPU ici.
* Compréhension par LLM : client prêt, jamais exercé sur un vrai modèle.
* Validation empirique sur des mesures réelles : démontrée seulement sur des
  observations synthétiques étiquetées comme telles.
* Génération d'une physique inconnue sans équations ni données.
* Exécution de FMU, EDP, multicorps GPU, distribution multi-nœuds réelle.
* Auto-amélioration du code de SIMORPH.

## 5. Reproduire les mesures

```
scripts/setup-python.sh                       # SciPy, SymPy, Z3, MuJoCo, Gymnasium, PyTorch
cargo test --workspace
simorph improve run --target pipe             # boucle d'amélioration, conduite
simorph improve run --target pendulum
simorph discover kepler
simorph demo nanosat
simorph understand "<énoncé>"                 # compréhension FR/EN
simorph agents run "<énoncé>"                 # session multi-agents
simorph agents replay <trace.jsonl>
simorph campaign run <problème.json> --workers 2
```

## 6. Dernier passage complet des tests

Voir la fin de ce document, mise à jour à chaque passage complet.
