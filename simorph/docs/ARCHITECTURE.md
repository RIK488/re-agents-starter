# SIMORPH — Architecture globale (20 sous-systèmes)

Version 2 (2026-10-27). Remplace la version 1 (12 blocs ARC-01…ARC-12),
dont les identifiants restent valides et sont repris ci-dessous.

Ce document décrit le **système final**. L'état réel de chaque bloc
(opérationnel, expérimental, planifié, recherche) n'est jamais affirmé ici :
il est tenu dans `STATUS.md`, alimenté par les tests et par l'audit
`audit/AUDIT-2026-10-27.md`.

Documents associés :

| Document | Contenu |
|---|---|
| `COMPONENT_MAP.md` | graphe réel des dépendances entre crates (généré par `cargo metadata`) |
| `INTERFACE_CONTRACTS.md` | contrats d'interface entre tous les sous-systèmes |
| `OSS_INTEGRATION_MATRIX.md` | dépôts open source : faits vérifiés, décision, état d'intégration mesuré |
| `GAP_ANALYSIS.md` | capacités manquantes par rapport à la cible |
| `INFRASTRUCTURE.md` | déploiement local, cluster, cloud |
| `COMPUTE_STRATEGY.md` | exécution et entraînement CPU/GPU |
| `VALIDATION_FRAMEWORK.md` | dispositif de validation scientifique et technique |
| `IMPLEMENTATION_PLAN.md` | plan de réalisation de l'architecture complète |

## 1. Principes

1. **Rust au centre, pas partout.** Rust porte l'orchestration, les contrats,
   la concurrence, l'ordonnancement, l'isolation, la sécurité, la provenance,
   les services et les noyaux numériques dont la performance ou la sûreté se
   mesure. Python, Julia, C++ et CUDA restent la voie d'accès aux bibliothèques
   matures (SciPy, MuJoCo, PyTorch, Z3, SymPy…), toujours derrière le même
   contrat de composant et dans un processus isolé.
2. **Un seul contrat d'exécution.** Tout moteur — natif, Python, FMU, worker
   distant, modèle d'IA entraîné — est un `Component` décrit par un
   `ComponentManifest` (§7 du cahier des charges).
3. **Niveaux de preuve non interchangeables.** Calculé, simulé, validé
   empiriquement, démontré, hypothèse : transitions restreintes, toujours
   adossées à un enregistrement de validation. `MathematicallyProven` exige un
   certificat vérifié indépendamment (sous-système 13).
4. **Juge indépendant et protégé.** Validation, suites d'évaluation scellées,
   critères de réussite hachés et autorisations de déploiement ne sont
   modifiables ni par le directeur, ni par les agents, ni par la boucle
   d'auto-amélioration (Admin + approbation humaine signée).
5. **Tout est tracé.** Chaque problème, hypothèse, run, jeu de données,
   validation, décision et modification est un nœud d'un registre chaîné.

## 2. Vue en couches

```
 ┌─────────────────────────────────────────────────────────────────────────────┐
 │ 18 Interface de laboratoire (web, CLI) · visualisation 2D/3D · suivi direct │
 └──────────────┬──────────────────────────────────────────────────────────────┘
                │ actions (registre unique CLI = HTTP)
 ┌──────────────▼──────────────────────────────────────────────────────────────┐
 │ 2 Orchestrateur multi-agents : Interprète · Planificateur · Modélisateur ·  │
 │   Expérimentateur · Critique · Validateur (indépendant) · Archiviste        │
 ├─────────────────────────────────────────────────────────────────────────────┤
 │ 3 Compréhension & décomposition │ 4 Générateur de modèles │ 5 Générateur de │
 │   (langue naturelle → ProblemSpec)│  (équations, chaînage) │   simulateurs   │
 ├─────────────────────────────────────────────────────────────────────────────┤
 │ 9 Expérimentation autonome (directeur scientifique + runtime de campagne)   │
 │ 10 Évolution d'algorithmes/agents │ 11 Entraînement │ 12 Renforcement       │
 │ 20 Boucle d'amélioration continue (limites → données validées → entraînement│
 │    → comparaison scellée → acceptation/rejet → retour arrière)              │
 ├─────────────────────────────────────────────────────────────────────────────┤
 │ 6 Cosimulation │ 8 Calcul scientifique & physique │ 7 Multimodal │ 19 Connecteurs│
 ├─────────────────────────────────────────────────────────────────────────────┤
 │ 13 Vérification formelle │ 14 Évaluation indépendante │ 15 Mémoire & traçabilité│
 ├─────────────────────────────────────────────────────────────────────────────┤
 │ 1 Noyau Rust : contrats · unités · ordonnanceur · exécution · sécurité ·    │
 │   télémétrie │ 16 Calcul distribué CPU/GPU │ 17 Génération/exécution sûre    │
 └─────────────────────────────────────────────────────────────────────────────┘
```

## 3. Les vingt sous-systèmes

Pour chaque bloc : responsabilités, réalisation (crates), technologies
externes, contrats principaux. Les états sont dans `STATUS.md`.

### 1. Noyau Rust haute performance
* Responsabilités : contrats universels, unités et dimensions, niveaux de
  preuve, identifiants, ordonnancement de graphes de tâches sous contrainte
  de ressources, exécution (in-process, sous-processus), sécurité, télémétrie.
* Crates : `simorph-core`, `simorph-units`, `simorph-scheduler`,
  `simorph-execution`, `simorph-orchestrator`, `simorph-plugin-sdk`,
  `simorph-security`, `simorph-telemetry`.
* Technologies : Tokio, Rayon, Serde, Axum, nalgebra, tracing (ADR-0001).

### 2. Orchestrateur autonome multi-agents
* Responsabilités : agents à rôles et permissions explicites, bus de messages
  typés et journalisés, superviseur (budget, détection de boucles), règles
  non contournables (aucun agent ne modifie critères, références, permissions).
* Crate : `services/agents`. Raisonnement LLM optionnel derrière un client
  configurable, sans autorité sur les niveaux de preuve.
* Réutilisation possible : LangGraph (graphe d'agents Python exposé comme
  `Component`), OpenHands pour les tâches de développement en conteneur.

### 3. Compréhension et décomposition des problèmes
* Responsabilités : texte → `ProblemSpec` (grandeurs, unités, rôles,
  objectifs, contraintes, inconnues), correspondance avec les ports des
  composants, liste explicite des ambiguïtés ; décomposition en catégories
  (connu, inconnu, calculable, à modéliser, à simuler, données, preuve,
  vérification).
* Crates : `services/problem-understanding`, `services/scientific-director`
  (analyse).

### 4. Générateur de modèles scientifiques et mathématiques
* Responsabilités : descripteurs de modèles (16 formalismes), chaînage de
  composants par unités, modèles exécutables à partir d'équations typées,
  découverte de lois par évolution symbolique (via 10), vérification
  symbolique (via 13).
* Crates : `simorph-model-registry`, `services/model-factory`,
  `simorph-evolution` (régression symbolique dimensionnelle).
* Réutilisation : SymPy, OpenModelica (Modelica → FMU).

### 5. Générateur de simulateurs spécialisés et multidisciplinaires
* Responsabilités : pipeline en 10 étapes (recherche, compatibilité,
  configuration, intégration, complément, génération, test, comparaison de
  méthodes, amélioration, documentation des limites).
* Crate : `services/simulator-factory`.

### 6. Cosimulation et couplage
* Responsabilités : esclaves type FMI, maîtres Jacobi/Gauss-Seidel/itératif,
  conversion d'unités, boucles algébriques, ensembles, FMU externes.
* Crates : `simorph-cosimulation`, `adapters/fmi`.
* Réutilisation : FMI 2/3, FMPy, preCICE (couplage de champs, phase ultérieure).

### 7. Infrastructure multimodale
* Responsabilités : actifs typés (texte, image, audio, vidéo, géométrie,
  tableaux) avec empreinte, analyses produisant des grandeurs à unités,
  géométrie maillée (volume, inertie, étanchéité).
* Crate : `simorph-multimodal` (image, hound, rustfft, ffmpeg en CLI).
* Cible : modèles multimodaux (vision, audio) via le pont Python (11).

### 8. Calcul scientifique et simulation physique
* Responsabilités : intégrateurs explicites et implicites, événements,
  évènements discrets, multiagents, modèles physiques de domaine, moteurs
  externes matures.
* Crates : `simorph-simulation`, `plugins/physics`, `plugins/engineering`,
  `adapters/scientific-python` (SciPy, SymPy, MuJoCo, Gymnasium, PyTorch).
* Réutilisation : SciPy (raides/DAE), MuJoCo (corps rigides), Chrono et
  Warp (phase ultérieure), SUNDIALS/PETSc/FEniCSx (EDP, phase ultérieure).

### 9. Expérimentation scientifique autonome
* Responsabilités : analyse, hypothèses réfutables, programme expérimental à
  critères figés et hachés, exécution, révision, contradictions, arrêt,
  conclusions traçables ; plans d'expériences, reproduction, contre-exemples.
* Crates : `services/scientific-director`, `services/campaign-runtime`,
  `simorph-experiment`, `simorph-optimization`, `simorph-uncertainty`.

### 10. Évolution des algorithmes et des agents
* Responsabilités : programmation génétique typée et dimensionnelle, îlots,
  diversité de qualité (MAP-Elites), évolution de code source exécuté en bac
  à sable avec opérateurs de mutation syntaxiques ou LLM, évaluateur scellé.
* Crate : `simorph-evolution`.
* Inspirations : ShinkaEvolve, Darwin Gödel Machine (principes repris,
  licences et sûreté vérifiées dans la matrice).

### 11. Entraînement et fine-tuning des modèles d'IA
* Responsabilités : réseaux natifs déterministes (MLP, Adam), modèles comme
  composants, jobs d'entraînement in-process / Python / externe, jeux de
  données hachés, gabarits PyTorch/TRL/PEFT, gestion GPU (16).
* Crates : `simorph-learning`, `services/training-service`.
* Réutilisation : PyTorch, Transformers, PEFT, TRL, DeepSpeed (GPU).

### 12. Apprentissage par renforcement
* Responsabilités : environnements (natifs et Gymnasium), REINFORCE, PPO,
  Q-learning, contrôle des simulateurs SIMORPH comme environnements.
* Crates : `simorph-learning` (RL), `simorph-optimization::rl`,
  `adapters/scientific-python` (Gymnasium).
* Réutilisation : Ray RLlib (passage à l'échelle).

### 13. Vérification mathématique et formelle
* Responsabilités : SMT (Z3) avec certificats re-vérifiés indépendamment,
  vérification symbolique (SymPy), arithmétique d'intervalles à arrondi
  dirigé, back-end Lean 4, octroi de `MathematicallyProven` aux seuls énoncés
  certifiés.
* Crate : `simorph-formal`, en lien avec `simorph-validation`.

### 14. Évaluation indépendante des performances
* Responsabilités : suites scellées protégées (Admin + approbation), fiches
  de score séparant qualité, robustesse, généralisation, vitesse et coût,
  harnais externes.
* Crates : `services/evaluation-service`, `services/ai-laboratory`,
  `simorph-validation`.
* Réutilisation : Inspect AI, lm-evaluation-harness, HELM, LightEval.

### 15. Mémoire scientifique persistante et traçabilité
* Responsabilités : registre de provenance chaîné et vérifiable, archive
  d'expériences, graphe de connaissances typé par niveau de preuve, mémoire
  §11, explication causale, transferts interdisciplinaires comme hypothèses.
* Crates : `simorph-provenance`, `simorph-knowledge`,
  `services/knowledge-service`, `simorph-experiment::archive`.
* Réutilisation : PostgreSQL (registre multi-utilisateur), DuckDB, MLflow et
  DVC (suivi d'expériences et versionnement de données), Qdrant (recherche
  vectorielle).

### 16. Calcul distribué CPU/GPU
* Responsabilités : coordinateur et workers, inventaire matériel réel,
  placement selon ressources, battements de cœur, reprise sur panne,
  déduplication, authentification des workers ; correspondance avec Ray et
  les Jobs Kubernetes.
* Crate : `simorph-distributed` ; voir `COMPUTE_STRATEGY.md`.

### 17. Génération, compilation et exécution sécurisée de code
* Responsabilités : exécution de code généré en sous-processus avec limites
  système réelles (rlimits CPU, mémoire, fichiers, processus), espace de noms
  réseau isolé, environnement nettoyé, répertoire jetable, délais, plafonds
  de sortie ; compilation contrôlée ; conteneurs en production.
* Crates : `simorph-execution`, `simorph-security`.

### 18. Interface interactive de laboratoire
* Responsabilités : espaces Projet, Recherche, Modélisation, Simulation,
  Optimisation, IA, Validation, Journal, Export ; visualisation Plotly et
  Three.js ; suivi en direct ; seules les actions implémentées sont visibles.
* Crates : `apps/web`, `apps/cli`.

### 19. Connecteurs vers données, instruments et environnements réels
* Responsabilités : sources de données à unités (CSV, JSON-lines, HTTP),
  instruments (lecture, consigne protégée par approbation humaine), rejeu
  d'enregistrements, transport série/SCPI, observations pour calibration.
* Crate : `simorph-connectors`.

### 20. Boucle d'amélioration continue
* Responsabilités : identifier les limites d'un modèle, concevoir les
  expériences, produire et valider les données, entraîner sous contrôle,
  comparer avant/après sur suite scellée, conserver les gains démontrés,
  rejeter les dégradations, revenir en arrière ; témoin négatif obligatoire.
* Crates : `services/improvement-loop`, `services/ai-laboratory`.

## 4. Flux principal

1. Texte, fichiers ou `ProblemSpec` entrent par l'interface (18).
2. L'Interprète (3) produit un `ProblemSpec` et ses ambiguïtés.
3. Le Planificateur (9) analyse, formule les hypothèses et fige les critères.
4. Le Modélisateur (4, 5) assemble ou génère les modèles et simulateurs.
5. L'Expérimentateur (9) exécute via le noyau (1), localement ou sur workers (16).
6. Le Validateur (13, 14) accorde les niveaux de preuve ; personne d'autre.
7. Le Critique (2) propose réplications, haute fidélité ou reformulation.
8. L'Archiviste (15) alimente la mémoire ; la boucle (20) exploite les
   données validées pour améliorer les modèles d'IA.

## 5. Frontières de confiance

| Zone | Contenu | Peut modifier |
|---|---|---|
| Protégée | suites scellées, références de validation, critères hachés, politique RBAC, clés | Admin + approbation humaine |
| Noyau | crates Rust, services | revue humaine (git) |
| Bac à sable | code généré, scripts Python, programmes évolués | rien hors de son répertoire jetable |
| Externe | instruments, API, déploiements | uniquement avec `ExternalSideEffect` approuvé |
