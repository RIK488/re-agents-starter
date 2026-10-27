# SIMORPH — Architecture cible intégrale

Statut : architecture de référence couvrant **l'intégralité du système final**.
Chaque composant est repéré par un identifiant stable (`ARC-xx`) repris dans
`REQUIREMENTS.md`, `ACCEPTANCE_CRITERIA.md` et `STATUS.md`. Le registre d'état
(`STATUS.md`) est la seule source de vérité sur ce qui est réellement
implémenté et testé ; ce document décrit la cible, pas l'état d'avancement.

## 1. Principes directeurs

| Principe | Conséquence architecturale |
|---|---|
| Aucun domaine codé en dur dans le noyau | Le noyau (`crates/`) ne connaît que des manifestes, des unités, des graphes de tâches et des niveaux de preuve. Les disciplines vivent dans `plugins/` et dans des composants externes décrits par le contrat §7. |
| Catégories épistémiques non interchangeables | `EvidenceLevel` (calculé / simulé / validé empiriquement / démontré / hypothèse) est porté par chaque `TaskResult` et chaque `Claim`. Les transitions sont restreintes et exigent un enregistrement de validation (`simorph-core::evidence`). |
| Le directeur ne juge pas ses propres résultats | Le moteur de validation est un crate séparé, sans dépendance vers le directeur ; les références de validation sont scellées (`ProtectedReferences`, `ProtectedGuard`) et toute mutation exige un principal Admin **et** une approbation humaine. |
| Rust là où il apporte une valeur démontrable | Orchestration, ordonnancement, exécution, provenance, sécurité, télémétrie, noyaux numériques critiques en Rust ; bibliothèques scientifiques Python/Julia/C++/CUDA intégrées par adaptateurs, jamais réécrites sans raison technique démontrée (`research/open-source-audit/`). |
| Provenance complète | Tout run possède un identifiant unique, un hash de requête, un hash de sortie, et un enregistrement chaîné dans le registre de provenance avec capture de l'environnement. |
| Simulation guidée par l'incertitude | Toute décision d'expérience est scorée par un `Tradeoff` (précision, temps, coût, risque, valeur informative) avec des poids déclarés par le propriétaire du problème, pas par le directeur. |

## 2. Vue d'ensemble

```
                       ┌────────────────────────────────────────────┐
                       │  Interface universelle (ARC-01)            │
                       │  apps/web (API HTTP + UI)  apps/cli         │
                       └───────────────┬────────────────────────────┘
                                       │ ProblemSpec / commandes
                       ┌───────────────▼────────────────────────────┐
                       │  Directeur scientifique autonome (ARC-02)  │
                       │  services/scientific-director              │
                       │  décomposition · hypothèses · programme    │
                       │  expérimental · arrêt · conclusions        │
                       └───┬──────────┬──────────┬──────────┬───────┘
                           │          │          │          │
     ┌─────────────────────▼──┐ ┌─────▼──────┐ ┌─▼────────┐ ┌▼──────────────────┐
     │ Modélisation (ARC-03)  │ │ Expériment.│ │ Optimis. │ │ Laboratoire IA    │
     │ model-registry,        │ │ (ARC-06)   │ │ (ARC-07) │ │ (ARC-10)          │
     │ model-factory          │ │ experiment │ │ optimiz. │ │ ai-laboratory,    │
     └─────────┬──────────────┘ └─────┬──────┘ └─┬────────┘ │ training, eval    │
               │                      │          │          └───────┬───────────┘
     ┌─────────▼──────────────────────▼──────────▼──────────────────▼───────────┐
     │ Simulation (ARC-04/05) : simulation, cosimulation, simulator-factory,     │
     │ plugins/*, adaptateurs python / fmi / subprocess / http / container        │
     └─────────┬────────────────────────────────────────────────────────────────┘
               │ TaskRequest / TaskResult (contrat §7)
     ┌─────────▼────────────────────────────────────────────────────────────────┐
     │ Noyau d'exécution Rust (ARC-11) : orchestrator · scheduler · execution ·  │
     │ security · telemetry · provenance                                        │
     └─────────┬────────────────────────────────────────────────────────────────┘
               │
     ┌─────────▼──────────────┐   ┌──────────────────────────┐   ┌───────────────────┐
     │ Validation indépendante │   │ Incertitude (ARC-09)     │   │ Mémoire / savoir  │
     │ (ARC-08) validation     │   │ uncertainty              │   │ (ARC-12) knowledge│
     └────────────────────────┘   └──────────────────────────┘   └───────────────────┘
```

Le directeur **ne dépend pas** de la validation ; la validation **ne dépend pas**
du directeur. Les deux ne partagent que `simorph-core`. C'est une contrainte de
compilation (graphe de dépendances Cargo), pas une convention.

## 3. Sous-systèmes

### ARC-01 Interface universelle (`apps/web`, `apps/cli`)
Responsabilités §3.1 et §12. Chaque commande visible correspond à une action
réellement implémentée : l'API HTTP expose uniquement des routes câblées à des
fonctions testées, et l'interface liste les actions à partir du même registre
que le CLI (`apps/cli/src/commands.rs`). Espaces : Projet, Recherche,
Modélisation, Simulation, Optimisation, IA, Validation, Journal/Provenance,
Export. Visualisation 2D/3D : séries temporelles (Plotly), trajectoires 3D
(Three.js) alimentées par les trajectoires JSON exportées par le noyau.

### ARC-02 Directeur scientifique autonome (`services/scientific-director`)
Machine à états d'une **campagne** :
`Intake → Analyse(connus/inconnus/calculable/à modéliser/à simuler/données manquantes/à démontrer/à vérifier) → Hypothèses → Programme expérimental → Exécution → Révision → Décision (continuer / arrêter / conclure)`.
Chaque transition est enregistrée dans la provenance. Les critères d'arrêt
(budget, absence de gain informatif, contradictions non résolues) et les poids
du compromis viennent du `ProblemSpec`. Le raisonnement en langage naturel
(LLM) est un **composant optionnel** derrière une interface `Reasoner` ; le
directeur fonctionne sans lui avec un planificateur déterministe fondé sur les
manifestes (capacités, domaines, unités) et sur les règles de sélection de
méthodes. Un LLM ne peut jamais promouvoir un niveau de preuve.

#### Runtime de campagne (`services/campaign-runtime`)
Le directeur ne dépend d'aucun moteur : il émet des `PlannedExperiment` et
consomme des `ExperimentOutcome`. Le runtime de campagne implémente
`ExperimentExecutor` avec les moteurs réels (expérimentation, optimisation
avec sélection de méthode, incertitude), soumet chaque run à la validation
indépendante, convertit les unités problème ↔ ports, et journalise tout dans
la provenance. C'est lui, et non le directeur, qui évalue les critères de
réussite figés (hachés) avec l'évaluateur d'expressions du moteur de
validation.

### ARC-03 Moteur universel de modélisation (`crates/simorph-model-registry`, `services/model-factory`)
`ModelDescriptor` pour chaque formalisme listé §3.3. La fabrique de modèles
construit des descripteurs à partir d'un `ProblemSpec` et des composants
disponibles (chaînage par unités et capacités), et vérifie la cohérence
dimensionnelle avant tout calcul.

### ARC-04 Générateur autonome de simulateurs (`services/simulator-factory`)
Pipeline en 10 étapes (§3.4) : recherche dans le registre → compatibilité →
configuration → intégration (adaptateur) → complément → génération (composition
de modèles existants, génération de code Python/Rust pour un composant
subprocess, exécuté en bac à sable) → tests de validité (cas de référence,
invariants) → comparaison de méthodes numériques (plusieurs solveurs sur le même
modèle) → amélioration (sélection du solveur le plus rapide à précision
donnée) → documentation des limites (manifeste). Un simulateur généré reçoit le
niveau `Simulated` et ne devient jamais « validé » sans confrontation à des
références scellées.

### ARC-05 Cosimulation multidisciplinaire (`crates/simorph-cosimulation`, `adapters/fmi`)
Interface esclave de type FMI, algorithmes maîtres Jacobi / Gauss-Seidel /
itératif avec retour arrière, conversion d'unités automatique, détection de
boucles algébriques, propagation d'incertitude par ensembles, détection
d'incohérences (NaN, sortie de domaine de validité, plages de ports).

### ARC-06 Expérimentation universelle (`crates/simorph-experiment`)
Plans d'expériences, exécution parallèle, reproduction par identifiant,
archivage, recherche de contre-exemples, analyse des causes probables d'échec.

### ARC-07 Optimisation (`crates/simorph-optimization`)
Onze familles listées §3.7 ; `MethodSelector` choisit en fonction des traits
du problème et justifie son choix par écrit.

### ARC-08 Validation scientifique indépendante (`crates/simorph-validation`)
Les onze contrôles §3.8 et la classification épistémique. Les références
sont scellées ; la clé de descellement est une approbation humaine.

### ARC-09 Incertitude (`crates/simorph-uncertainty`)
Propagation, sensibilité globale (Sobol, Morris), valeur de l'information,
sélection de fidélité, surrogates.

### ARC-10 Laboratoire d'amélioration des IA (`services/ai-laboratory`, `training-service`, `evaluation-service`, `plugins/ai`)
Boucle §5.1 implémentée comme une campagne du directeur spécialisée : les
candidats d'amélioration sont évalués par `evaluation-service` contre des
suites de référence scellées, acceptés uniquement si l'amélioration est
statistiquement significative sur la métrique déclarée **et** sans régression
sur les métriques protégées. Le `training-service` pilote PyTorch/TRL/PEFT via
le pont Python (`adapters/python`), jamais en in-process. L'auto-amélioration
de SIMORPH (§5.2) passe par le même circuit, en bac à sable, avec retour
arrière (`Rollback`) et sans accès aux références scellées ni à la politique de
sécurité (`Permission::SelfModify` interdite aux agents autonomes).

### ARC-11 Noyau d'exécution Rust (`orchestrator`, `scheduler`, `execution`, `security`, `telemetry`, `provenance`, `plugin-sdk`)
Voir `adr/0001-choix-technologiques-rust.md`. Graphe de tâches, admission par
ressources, délais, annulation, reprise après interruption, isolation des
composants externes (sous-processus avec politique de bac à sable), RBAC,
approbation humaine, journal d'audit, métriques Prometheus/OpenTelemetry.

### ARC-12 Mémoire scientifique et transfert interdisciplinaire (`crates/simorph-knowledge`, `services/knowledge-service`)
Graphe de connaissances typé par niveau de preuve et par statut ; les
transferts entre disciplines sont produits comme **hypothèses** avec critère
de réfutation.

## 4. Flux de données

1. `ProblemSpec` (JSON, schéma `schemas/problem_spec.schema.json`) entre par
   l'interface.
2. Le directeur produit une `Campaign` : analyse, hypothèses, programme
   (liste d'`ExperimentPlan`), chaque plan ciblant un composant du registre.
3. L'orchestrateur transforme chaque plan en `RunGraph` (nœuds = `TaskRequest`),
   l'ordonnanceur les exécute sous contrôle de ressources ; chaque résultat est
   un `TaskResult` avec `request_hash`, `output_hash`, `evidence`.
4. Le moteur de validation produit un `ValidationRecord` par run/expérience ;
   seul lui peut faire évoluer le `EvidenceLevel` d'un `Claim`.
5. Provenance : chaque objet ci-dessus est un nœud chaîné ; `explain(id)`
   reconstruit la chaîne causale.
6. Export : rapports Markdown/JSON, trajectoires, manifestes, code généré.

## 5. Contrat universel (§7)

`simorph_core::ComponentManifest` est la définition normative ; le schéma JSON
est exporté dans `schemas/component_manifest.schema.json`. Les adaptateurs
(`adapters/`) produisent ce manifeste pour les outils qui ne peuvent pas le
fournir eux-mêmes (FMU, scripts Python, services HTTP).

## 6. Infrastructure (§10)

* Local : `cargo run -p simorph-cli`, `cargo run -p simorph-web` ; stockage
  JSON-lines/DuckDB ; modèles locaux via llama.cpp/vLLM derrière l'interface
  `Reasoner`.
* Distribué : ordonnanceur multi-nœuds via Ray (adaptateur) ou Kubernetes
  Jobs ; files de travaux persistées ; reprise par `snapshot()/resume()`.
* Cloud : manifestes `infrastructure/` (docker-compose, Kubernetes), stockage
  objet pour artefacts, PostgreSQL pour le registre, Qdrant pour la mémoire
  vectorielle, OpenTelemetry pour l'observabilité, gestion des secrets par
  variables d'environnement injectées (jamais dans le code).

## 7. Sécurité (§10.4)

Modèle complet dans `SECURITY_MODEL.md`. Invariants vérifiés par tests :
un agent autonome n'obtient jamais `ExternalSideEffect` ni `SelfModify` sans
approbation humaine signée ; les références protégées sont immuables sans
Admin + approbation ; le code généré s'exécute en sous-processus avec délai,
plafond de sortie, environnement nettoyé, puis en conteneur en production.

## 8. Ce qui reste de la recherche

`RESEARCH_REQUIREMENTS.md` liste les capacités exigées par le cahier des
charges qui ne sont pas scientifiquement acquises (génération autonome fiable
de simulateurs pour des physiques nouvelles, auto-amélioration démontrée des
capacités de raisonnement, vérification formelle automatique de résultats
numériques, etc.). Elles sont des exigences de recherche documentées, jamais
présentées comme acquises.
