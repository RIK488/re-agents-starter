# SIMORPH — Registre d'état (source de vérité)

Dernière mise à jour : 2026-10-27. Généré à partir de l'arbre de sources ; les
nombres de tests proviennent de `cargo test --workspace` (tous verts, 0
avertissement de compilation).

Légende : **✅ implémenté et testé** (tests automatisés nommés) · **🟡 partiel**
(fonctionne dans le périmètre indiqué, limites listées) · **📋 planifié**
(architecture et exigence définies, pas de code) · **🔬 recherche** (capacité
non acquise scientifiquement, voir `RESEARCH_REQUIREMENTS.md`).

Rien dans ce registre n'est déclaré opérationnel sans test.

## 1. Noyau et contrats

| Sous-système | État | Crate / tests | Limites |
|---|---|---|---|
| Contrats universels (ids typés, niveaux de preuve non interchangeables, `ComponentManifest` §7, `ProblemSpec`, `TaskRequest/Result`, annulation/délais, compromis §8) | ✅ | `simorph-core` (14 tests) | — |
| Analyse dimensionnelle, conversion d'unités | ✅ | `simorph-units` (6 tests : SI, préfixes, composés, °C/°F, incompatibilités) | exposants rationnels limités à n/2 |
| Schémas JSON exportés | ✅ | `schemas/*.json` (10 fichiers), `simorph schemas export` | — |

## 2. Moteurs scientifiques natifs

| Sous-système | État | Crate / tests | Limites |
|---|---|---|---|
| Simulation : Euler, RK4, RK45 adaptatif, Euler implicite, trapèze implicite, événements (zéro-crossing), simulation à événements discrets, ordonnanceur multiagents, `OdeComponent` | ✅ | `simorph-simulation` (13 tests : analytiques, ordres de convergence mesurés, raide, rebond, annulation) | pas d'EDP/DAE natifs (délégués : SUNDIALS/FEniCSx via adaptateurs, 📋) |
| Cosimulation : esclaves type FMI, Jacobi, Gauss-Seidel, GS itératif avec retour arrière, conversion d'unités, boucles algébriques, doublement de pas, ensembles | ✅ | `simorph-cosimulation` (7 tests : équivalence monolithique, unités, boucle algébrique, NaN nommé) | stabilité a priori non analysée (🔬 RR-7) |
| Expérimentation : plans (factoriel, LHS, aléatoire, OAT, composite centré, Plackett-Burman), exécution parallèle déterministe, reproduction par id, archive, contre-exemples, causes d'échec | ✅ | `simorph-experiment` (7 tests) | causes = association statistique, pas causalité (🔬 RR-4) |
| Optimisation : Nelder-Mead, descente de gradient, L-BFGS, DE, CMA-ES, recuit, NSGA-II + hypervolume, bayésienne (GP/EI), MCTS, faisceau, robuste (espérance/CVaR), Q-learning tabulaire, sélecteur de méthode motivé | ✅ | `simorph-optimization` (30 tests sur bancs de référence) | RL à grande échelle et NAS via adaptateurs (📋) |
| Incertitude : échantillonnage (MC, LHS, Sobol, Halton, Iman-Conover), propagation, Sobol/Morris, sensibilités locales, EVPI, réduction de variance attendue, sélecteur de fidélité, surface de réponse quadratique | ✅ | `simorph-uncertainty` (25 tests, Ishigami ±0,05) | pas de chaos polynomial complet |
| Validation : 11 contrôles (dimensionnel, invariants, Richardson, convergence, reproductibilité, sensibilité, z/χ², couverture, cas de référence, généralisation), classification épistémique, références scellées | ✅ | `simorph-validation` (17 tests) | certificats de preuve enregistrés, non vérifiés (🔬 RR-3) |
| Provenance : registre chaîné, capture d'environnement, JSONL vérifié, `explain` | ✅ | `simorph-provenance` (6 tests) ; round-trip exact des flottants garanti (`serde_json/float_roundtrip`, ADR-0002) | — |
| Registre de modèles : descripteurs 16 formalismes, découverte, compatibilité, chaînage, persistance, schémas | ✅ | `simorph-model-registry` (6 tests) | — |
| Connaissance : graphe typé par preuve/statut, contradictions, transferts = hypothèses, export mémoire §11 | ✅ | `simorph-knowledge` (4 tests) | transferts non évalués automatiquement (🔬 RR-5) |

## 3. Noyau d'exécution Rust

| Sous-système | État | Crate / tests | Limites |
|---|---|---|---|
| Ordonnanceur : DAG, ressources, priorités, délais, réessais, annulation en cascade, snapshot/reprise | ✅ | `simorph-scheduler` (7 tests) | mono-nœud |
| Exécution : in-process avec délai, composants sous-processus (protocole JSON-lines, bac à sable : délai, plafond de sortie, environnement nettoyé, redémarrage) | ✅ | `simorph-execution` (9 tests dont Python réel) | isolation réseau/FS/mémoire par conteneur (manifestes fournis), pas par le runner |
| Orchestrateur : graphes de runs, dépendances de données, supervision, métriques, superviseur | ✅ | `simorph-orchestrator` (5 tests) | — |
| SDK plugin : builder, `ComponentSet`, protocole, `serve_stdio` | ✅ | `simorph-plugin-sdk` (4 tests + test de bout en bout) | — |
| Sécurité : RBAC, approbation humaine signée obligatoire pour `ExternalSideEffect`/`SelfModify` des agents, bac à sable, secrets masqués, `ProtectedGuard`, journal d'audit | ✅ | `simorph-security` (13 tests) | authentification HTTP = clé API (🟡, OIDC 📋) |
| Télémétrie : traces, métriques, exposition Prometheus | ✅ | `simorph-telemetry` (5 tests) | export OTLP 📋 |
| Exécution distribuée (Ray / Jobs Kubernetes) | 📋 | manifestes `infrastructure/` | REQ-RUN-8 |

## 4. Adaptateurs

| Adaptateur | État | Tests | Limites |
|---|---|---|---|
| Pont Python (paquet `simorph_adapter`, scripts `python3 -I`, exemples NumPy/SciPy) | ✅ | 3 tests Rust + 2 tests Python, exécutés réellement | SciPy absent ici → repli NumPy signalé |
| FMI 2.0/3.0 : lecture de `modelDescription.xml`, manifeste, unités | ✅ | 4 tests | exécution via FMPy : `NotImplemented` explicite si `fmpy` absent (🟡) |
| HTTP/gRPC/WebAssembly/conteneur | 📋 | variantes `ExecutionInterface` définies | ADR-0001 |

## 5. Services

| Service | État | Tests | Limites |
|---|---|---|---|
| Directeur scientifique : analyse §2 (8 catégories), hypothèses réfutables, programme avec critères figés et hachés, campagne (révision, contradictions, arrêt, conclusions traçables), raisonneur optionnel sans autorité | ✅ | `scientific-director` (11 tests) | LLM : interface définie, client non livré (📋) |
| Runtime de campagne : moteurs réels + validation indépendante + provenance ; conversion d'unités problème↔ports | ✅ | `campaign-runtime` (1 test) + `tests/integration` (6 tests d'acceptation) | méthodes multiobjectif/discrètes via scalarisation (🟡) |
| Laboratoire IA : suites scellées, boucle §5.1, acceptation significative sans régression, vitesse ≠ raisonnement, retour arrière, auto-amélioration gardée | ✅ | `ai-laboratory` (5 tests) | cibles réelles (LLM, vision) via pont Python (🟡) |
| Évaluation : registre de suites protégé (Admin + approbation), fiche de score par type, harnais externes (Inspect AI, lm-eval, LightEval, HELM) | 🟡 | 6 tests (harnais factice de bout en bout ; harnais réels détectés absents) | installation des harnais hors dépôt |
| Entraînement : jobs in-process / Python / externe, jeux de données synthétiques hachés, gabarits PyTorch/TRL/PEFT | 🟡 | 4 tests (NumPy de bout en bout ; torch détecté absent) | GPU, torch non installés ici |
| Fabrique de modèles : assemblage par chaînage, descripteurs, `ExpressionModel` à partir d'équations typées avec inférence dimensionnelle | ✅ | 9 tests | EDO explicites et algébriques seulement |
| Fabrique de simulateurs : pipeline 10 étapes, génération Rust (équations) et Python (bac à sable, SHA-256), comparaison de 5 méthodes, documentation des limites | ✅ | 5 tests (dont génération Python réelle, échec explicite sans équations) | génération pour physiques nouvelles (🔬 RR-1) |
| Service de connaissance : ingestion campagnes/labo, échecs, corrections, ADR, `why`, mémoire §11 | ✅ | 4 tests | — |

## 6. Interface et plugins

| Élément | État | Tests | Limites |
|---|---|---|---|
| CLI `simorph` (29 actions) | ✅ | 6 tests + démos exécutées (`demo nanosat|beam|pipe`) | — |
| API HTTP + interface web (9 espaces §12, Plotly 2D, Three.js 3D, suivi en direct via provenance, annulation, export) | ✅ | 2 tests couvrant chaque route (AC-UI-1), clé API | documents/images joints : empreintes supportées, extraction de contenu 📋 |
| Plugins physique (orbite+J2, thermique, batterie, solaire, attitude, nanosat cosimulé) | ✅ | 14 tests analytiques | — |
| Plugins ingénierie/économie/opérations/biologie | ✅ | 8 tests | — |
| Plugins IA (classifieur, raisonneur borné, bandit) | ✅ | 4 tests | — |
| Plugins biologie/chimie/matériaux/télécoms avancés | 📋 | — | intégrations de l'audit (T5) |

## 7. Correspondance exigences → preuves

| Exigence | Preuve |
|---|---|
| REQ-UI-13 | `apps/cli/src/actions.rs::actions_are_implemented_and_unique`, `apps/web/tests/routes.rs` |
| REQ-DIR-1..12 | `scientific-director` tests, `tests/integration/tests/acceptance.rs::ac_e2e_1_nanosat_campaign` |
| REQ-DIR-13 | `campaign.rs::tampered_criteria_are_refused`, `simorph-validation::protected`, `evaluation-service` registre protégé |
| REQ-VAL-12/13 | `simorph-core::evidence`, `simorph-validation::classify`, `protected` |
| REQ-SEC-9 | `simorph-security::rbac` (agents refusés sans approbation), `ai-laboratory::code_level_self_improvement_is_refused` |
| REQ-AIL-3/4 | `ai-laboratory::faster_but_shallower_reasoner_is_rejected_and_deeper_accepted`, `acceptance.rs::ac_ail_3_speed_is_not_reasoning` |
| REQ-GEN-1..12 | `simulator-factory` tests (10 étapes, échec explicite) |
| REQ-COS-* | `simorph-cosimulation/tests/cosim.rs` |
| REQ-UNC-* | `simorph-uncertainty` tests |
| REQ-MEM-* / REQ-TRF-* | `simorph-knowledge`, `knowledge-service`, `acceptance.rs` (mémoire ingérée) |
| REQ-OSS-1/2 | `research/open-source-audit/` (82 fiches, 12 champs) |

## 8. Hors périmètre de cette livraison (déclaré, non masqué)

* Exécution distribuée multi-machines / multi-GPU (📋 T6).
* Clients LLM pour le raisonneur (📋 ; l'interface `Reasoner` est sans autorité par construction).
* Vérification mécanique de certificats de preuve (🔬 RR-3).
* Intégrations externes de l'audit (MuJoCo, SUNDIALS, PyTorch réel, Inspect AI…) : adaptateurs et détection présents, bibliothèques non installées dans l'environnement de build.
