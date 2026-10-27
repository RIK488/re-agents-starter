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
| Lignes de Rust | 71 776 |
| Lignes de Python (adaptateurs, moteurs scientifiques, formel) | 1 695 |
| Tests (`#[test]` et `#[tokio::test]`) | 480 déclarés : 476 exécutés par défaut, 4 essais longs ignorés (à lancer en mode release) |
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
| 6 | Cosimulation | Opérationnel (natif) | Jacobi, Gauss-Seidel, itératif avec retour arrière, boucles algébriques, conversion d'unités ; équivalence avec la solution monolithique ; entrées non connectées refusées sauf valeur par défaut explicite ; inspection de FMU (`simorph fmu inspect`) | FMU : lecture de `modelDescription.xml` seulement (FMPy non installé) ; stabilité non analysée a priori (**RR-7**) |
| 7 | Multimodal | Opérationnel (analyses) | images, signaux audio, géométrie, tableaux : grandeurs à unités, empreintes ; 14 tests | aucun modèle appris de vision ou d'audio (GPU requis) |
| 8 | Calcul scientifique | Opérationnel | natif : RK45 2,19 M pas/s, RK4 5,5 M pas/s, implicites ; SciPy : Robertson à 6·10⁻¹², Van der Pol μ = 1000, LP Wyndor exact ; MuJoCo : pendule θ₀ = 2,5 rad à 2·10⁻⁷ de l'intégrale elliptique ; SymPy | pas d'EDP (FEniCSx, PETSc), pas de Chrono ni de Warp |
| 9 | Expérimentation autonome | Opérationnel (composants existants) | programme à critères figés et hachés ; conclusions limitées aux runs faisables ; inconnues lues à la solution retenue ; calibration sur observations CSV et niveau « validé empiriquement » seulement si z-test et χ² passent ; nanosat : masse 0,4177 kg sous les trois contraintes, 52 runs sur 400, 21 s | « gain informatif » heuristique, non information-théorique ; contradictions signalées mais non relancées (**RR-6**) |
| 10 | Évolution d'algorithmes | Opérationnel | programmation génétique dimensionnelle, MAP-Elites, évolution de programmes par mutations syntaxiques évaluées par instances scellées | pas de mutations par LLM en conditions réelles |
| 11 | Entraînement | Opérationnel (CPU) | MLP natif (Adam) ; PyTorch CPU déterministe (même empreinte de poids sur deux exécutions) | pas de GPU ; pas de fine-tuning de modèles de langage (TRL, PEFT, DeepSpeed) |
| 12 | Renforcement | Opérationnel (petite échelle) | PPO et REINFORCE natifs ; CartPole Gymnasium recoupé avec l'implémentation native à 2,2·10⁻⁸ | pas de RL distribué (RLlib) ni d'environnements lourds |
| 13 | Vérification formelle | Opérationnel (énoncés bornés) | Z3 : preuves et réfutations certifiées, relancées à l'identique et confirmées par une méthode indépendante (intervalles, LP, énumération) ; seul ce chemin accorde « démontré mathématiquement » | Lean 4 absent (renvoie `NotImplemented` avec la cause) ; pas de preuve sur trajectoires flottantes (**RR-3**) |
| 14 | Évaluation indépendante | Expérimental | suites scellées, bootstrap apparié, métriques protégées, témoins négatifs (étiquettes corrompues, simulateur bogué) rejetés ; `simorph evaluation status` vérifie les sceaux des suites standard et détecte les harnais réellement importables | aucun harnais externe utilisable ici : Inspect AI 0.3.285 s'installe mais exige un téléchargement `tiktoken` bloqué par le proxy, même avec son modèle factice ; aucun modèle de langage évalué |
| 15 | Mémoire scientifique | Opérationnel (local) | provenance chaînée ; en mode à clé, modification avec recalcul et troncature détectées (ancre de tête) ; ajout en temps constant (10 000 ajouts JSONL à clé : 2,5 s en debug, relecture et vérification : 1,35 s) ; graphe de connaissances ; archive d'expériences reproductibles par identifiant | pas de PostgreSQL ni de base vectorielle ; transferts interdisciplinaires non évalués automatiquement (**RR-5**) |
| 16 | Distribué | Opérationnel (TCP, un hôte) | coordinateur et workers authentifiés par HMAC, dé-duplication par empreinte de requête, reprise sur panne ; campagne répartie identique à l'exécution locale | pas de GPU, ni Ray, ni Jobs Kubernetes, ni chiffrement du transport ; un seul hôte physique testé |
| 17 | Génération et exécution sûres | Opérationnel | espace de noms réseau vide, `RLIMIT_DATA`, `RLIMIT_CPU`, `RLIMIT_FSIZE`, `RLIMIT_CORE`, groupe de processus tué, environnement nettoyé, plafond de sortie, échec fermé si l'isolation est indisponible (4 tests d'isolation) ; expressions évaluées en Python sur liste blanche | pas de restriction du système de fichiers (montages) ni de limite de processus (cgroups) ; sondes d'attaque dynamiques non exécutées |
| 18 | Interface de laboratoire | Opérationnel | CLI et API HTTP ; chaque action visible correspond à une route réelle (test) ; rôles par clé API, lecture seule pour `Viewer`, CORS fermé par défaut ; panneaux langage naturel, agents, formel, amélioration, découverte ; énoncé français résolu de bout en bout par l'API, validé et rejoué à l'identique (test) | pas d'OIDC ni de multi-utilisateur réel ; pas de visualisation volumique |
| 19 | Connecteurs | Opérationnel (fichiers et protocoles) | CSV et JSON-lines à unités et empreintes, HTTP, rejeu, SCPI sur transport ; `simorph data inspect` (unités vérifiées, empreinte SHA-256, enregistrement en provenance ; côté web, les noms de fichiers ne deviennent jamais des chemins) ; écriture vers un instrument soumise à approbation humaine | aucun instrument physique branché ; aucune donnée mesurée réelle |
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
| S13 | évaluation et entraînement non reliés | partiellement : boucle d'amélioration reliée ; service d'évaluation atteint par `evaluation status` (sceaux, harnais) ; service d'entraînement toujours hors chemin | `apps/cli::evaluation::tests` |
| Nouveau | double conversion d'unités des variables de conception après la correction E6 (flèche de poutre 10× trop faible), détectée par le validateur indépendant des agents | corrigé | `campaign-runtime::tests::beam_campaign_runs_end_to_end_with_unit_conversion`, `agents::beam_from_french_text_end_to_end_and_replay` |

### 2.2 Moteurs (E1–E15)

Chaque défaut a des tests de non-régression nommés `audit_*` dans la crate
concernée. Ils n'ont pas été exécutés contre l'ancien code, car beaucoup
appellent des API nouvelles ; la colonne « avant » vient des sondes de
l'audit.

| # | Crate | Avant | Maintenant |
|---|---|---|---|
| E1, E2 | validation, core | un drapeau de preuve sans contrôle donnait « démontré » ; une origine empirique quelconque suffisait | « démontré » exige un contrôle `formal:` réussi et une transition permise ; origine empirique liée au cas ; les contrôles à zéro ne relèvent jamais le niveau |
| E3 | provenance | modification avec recalcul et troncature indétectables ; ajout en O(n) | mode à clé HMAC sur chaque enregistrement et sur une ancre de tête ; séquence vérifiée au chargement ; ajout en temps constant. Sans clé, une modification cohérente reste indétectable (limite testée) |
| E4 | validation | références scellées modifiables avec nouveau hachage | sceaux à clé (`seal_keyed`, `verify_keyed`) |
| E5 | units | exposants fractionnaires faux, dénominateurs non normalisés, débordement | arithmétique d'exposants vérifiée, exposants rationnels, conversion dB refusée |
| E6 | optimization | unités, poids et caractère dur des contraintes ignorés par l'adaptateur | conversion entre unités du problème et des ports, poids appliqués, contraintes souples en pénalité |
| E7 | validation | constantes masquant des variables, NaN acceptés, débordement de pile | NaN refusés, profondeur d'expression bornée |
| E8 | validation | ordre de convergence « vérifié » avec 2 points | 3 points minimum |
| E9 | simulation | événement sur frontière de pas manqué, pas résiduel, erreur nulle annoncée par les implicites | détecté ; grille exacte ; indicateur `error_estimated` |
| E10 | cosimulation | boucle sur soi-même, dernier échantillon perdu, entrées non connectées à 0 | détectée ; dernier échantillon conservé ; entrées non connectées refusées |
| E11 | optimization | budgets dépassés (+47 %, ×2 pour la BO), NSGA-II sans budget, DE collée aux bornes, paniques sur NaN | budget dur compté sur chaque appel ; NSGA-II budgété ; essais hors bornes ramenés entre parent et borne ; NaN = pire valeur ; constante d'hypervolume ZDT1 corrigée (0,8767) |
| E12 | experiment | NaN comptés comme succès ; contre-exemples avalant les erreurs ; absence de critères = succès | NaN = échec ; erreurs rapportées ; absence de critères = non satisfait |
| E13 | uncertainty | graine ignorée par Sobol ; petits N silencieux ; intervalles échantillonnés en uniforme | Sobol brouillé par la graine ; taille minimale 4·(d+2) et avertissements ; intervalle échantillonné seulement sur demande explicite ; propagation d'intervalle déclarée non rigoureuse |
| E14 | model-registry | chaînage par nom sans dimension ; réenregistrement écrasant ; équations non vérifiées | nom et dimension ; doublon refusé (`replace_*` explicite) ; dimension des équations vérifiée, les équations non vérifiables signalées comme telles |
| E15 | knowledge | hypothèse « établie » sans validation | identifiant de validation exigé ; un test existant qui affirmait le défaut a été corrigé |

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

`cargo test --workspace --no-fail-fast`, 2026-10-27, profil debug, 4 cœurs :

| Mesure | Valeur |
|---|---|
| Binaires de test | 118 |
| Tests réussis | 476 |
| Tests échoués | 0 |
| Tests ignorés (essais longs) | 4 |
| Durée | 318 s |

Les tests qui exercent SciPy, SymPy, Z3, MuJoCo, Gymnasium et PyTorch
exigent l'environnement Python de `scripts/setup-python.sh`.
