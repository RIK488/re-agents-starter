# SIMORPH — Contrats d'interface entre sous-systèmes

Les contrats sont des types Rust (source de vérité) exportés en schémas JSON
dans `schemas/` (`simorph schemas export`). Un contrat franchit une frontière
de processus en JSON ; tout nombre non fini y est interdit (ADR-0002).

Notation : **P** = producteur, **C** = consommateur.

## 1. Contrats transverses (noyau, `simorph-core`)

| Contrat | Type | P → C | Invariants |
|---|---|---|---|
| Composant | `trait Component { manifest, check_compatibility, prepare, execute }` | tout moteur → noyau | `execute` honore `ExecutionControl` (annulation, délai) ; ne relève jamais son propre niveau de preuve |
| Manifeste | `ComponentManifest` (id, version, fournisseur, licence, capacités, domaines, ports à unités, hypothèses, validité, incertitude, précision, déterminisme, ressources, dépendances, interface d'exécution, protocole de validation, provenance, permissions) | composant → registre, directeur, validation | ports uniques par sens ; toute unité analysable par `simorph-units` |
| Requête / résultat | `TaskRequest` (entrées, graine, fidélité, configuration) / `TaskResult` (sorties, métriques, niveau revendiqué, `request_hash`, avertissements) | noyau ↔ composant | `request_hash` indépendant du `run_id` ; même requête + graine ⇒ même `output_hash` pour un composant déterministe |
| Problème | `ProblemSpec` (connus avec incertitudes, inconnues, variables de conception, objectifs, contraintes dures/souples, pièces jointes hachées, budget, poids du compromis) | interface, interprète → directeur | aucun infini ; poids du compromis fixés par le propriétaire |
| Niveau de preuve | `EvidenceLevel`, `Claim` | validation → tous | transitions restreintes (`allowed_transition`) |
| Unités | expressions texte, `Dimension`, `convert` | tous | conversion refusée entre dimensions différentes |
| Compromis | `Tradeoff`, `TradeoffWeights` | directeur → journal | heuristique documentée (non information-théorique) |

## 2. Contrats par sous-système

| # | Sous-système | Contrat principal | Crate | Entrées | Sorties |
|---|---|---|---|---|---|
| 1 | Noyau | `Scheduler::add/run/snapshot/resume`, `TaskSpec`, `ResourcePool` ; `Executor::run` ; `SubprocessComponent` + protocole JSON-lines (`adapters/PROTOCOL.md`) | scheduler, execution, plugin-sdk | tâches, requêtes | résultats, événements |
| 2 | Multi-agents | messages typés `AgentMessage` sur bus, rôles à permissions | `services/agents` | texte ou `ProblemSpec` | rapport, trace de messages |
| 3 | Compréhension | `UnderstandingResult { spec, mappings, ambiguities, unsupported_phrases }` | `services/problem-understanding` | texte FR/EN | `ProblemSpec` + ambiguïtés explicites |
| 3 | Décomposition | `analyse(&ProblemSpec, &dyn ComponentCatalog) -> ProblemAnalysis` | scientific-director | problème, catalogue | catégories §2, candidats, critères, problèmes dimensionnels |
| 4 | Modèles | `ModelDescriptor`, `ModelFactory::assemble -> ModelAssembly`, `ExpressionModel::new(equations, variables, parameters)` | model-registry, model-factory | problème, équations | descripteurs, chaîne, rapport dimensionnel |
| 4/10 | Découverte de lois | `Dataset`, `GpConfig`, `DimensionMode`, `discover(...) -> Discovery { verdict: LawVerdict, claim, validation, provenance }` | simorph-evolution | données à unités | loi + affirmation au plus `Simulated` |
| 5 | Simulateurs | `SimulatorRequest -> GenerationReport { steps[10], manifest, method_comparison, validation }` | simulator-factory | sorties requises, équations | composant documenté |
| 6 | Cosimulation | `trait CosimSlave { initialize, set_input, do_step, get_output, save/restore }`, `Coupling::builder().connect("a.out","b.in")`, `master::run(MasterOptions)` | cosimulation | esclaves, connexions | `CosimTrajectory` |
| 7 | Multimodal | `MediaAsset { kind, media_type, sha256, metadata }` et composants d'analyse | simorph-multimodal | fichiers | grandeurs à unités |
| 8 | Calcul | `trait DynamicalModel`, `simulate(model, params, x0, span, SolverOptions, ctl) -> Trajectory` ; moteurs externes via `adapters/scientific-python::spawn(name)` | simulation, scientific-python | modèle, paramètres | trajectoire, statistiques du solveur |
| 9 | Expérimentation | `PlannedExperiment` (critères hachés) ; `trait ExperimentExecutor { run(&PlannedExperiment) -> ExperimentOutcome }` ; `ExperimentPlan/Record`, `reproduce` | director, campaign-runtime, experiment | programme | résultats validés, conclusions |
| 10 | Évolution de programmes | `trait Mutator { propose(parent, feedback) -> Result<Vec<Program>> }`, `SealedInstances`, `evolve_programs` | simorph-evolution | programme graine | programme amélioré, évaluateur scellé |
| 11 | Entraînement | `Mlp`, `train(TrainConfig)`, `SurrogateComponent` ; `TrainingJob -> TrainingResult` ; `TorchMlpTrainer` | simorph-learning, training-service | jeu de données haché | modèle haché, métriques |
| 12 | Renforcement | `trait Environment { reset(seed), step(action) }`, `ppo`, `reinforce` | simorph-learning | environnement | politique, retours |
| 13 | Formel | `Problem { variables, bounds, hypotheses, goal }`, `prove/check_feasibility -> Certificate { smtlib, sha256, verdict, rerun_agrees, rechecks, externally_checked }`, `prove_claim` | simorph-formal | énoncé borné | certificat, `ValidationRecord` |
| 14 | Évaluation | `SealedSuite`, `Candidate`, `Evaluation`, `AcceptancePolicy`, `decide -> Decision` ; `ScoreCard` ; `ExternalHarness` | ai-laboratory, evaluation-service | candidat | évaluation, décision |
| 15 | Mémoire | `Ledger::append/record/verify_chain/explain`, `ProvenanceRecord` ; `KnowledgeGraph`, `TransferHypothesis` ; `ExperimentArchive` | provenance, knowledge, knowledge-service | événements | chaîne vérifiable, mémoire §11 |
| 16 | Distribué | coordinateur/worker JSON-lines sur TCP, `ResourceInventory`, authentification HMAC | simorph-distributed | `TaskRequest` | `Response` |
| 17 | Exécution sûre | `SandboxPolicy` (réseau, mémoire, CPU, fichiers, sortie, environnement, répertoire, échec fermé) → `isolation::apply` | security, execution | politique | processus isolé |
| 18 | Interface | registre `ACTIONS` (id, espace, libellé, commande CLI, route HTTP) ; API HTTP JSON ; rôles par clé | apps/cli, apps/web | actions | réponses JSON, tâches |
| 19 | Connecteurs | `trait DataSource` (schéma à unités, pagination, empreinte), `trait Instrument` (lecture ; consigne sous `ExternalSideEffect` + approbation) | simorph-connectors | sources, instruments | grandeurs horodatées, observations |
| 20 | Amélioration | `ImprovementConfig -> ImprovementReport` ; `SealedEvaluationSuite` ; `Variant` (dont témoins négatifs) | improvement-loop | cible (simulateur + modèle) | décisions, gains mesurés |

## 3. Contrats de confiance

| Contrat | Garantie | Détenteur |
|---|---|---|
| Critères de réussite hachés | une expérience évaluée avec d'autres critères est refusée (`Protected`) | directeur (création), runtime (évaluation) |
| Références et suites scellées | toute modification après scellement refusée ; empreinte dans chaque enregistrement ; sceau à clé disponible | validation, évaluation, boucle d'amélioration |
| `ValidationRecord` | seul producteur légitime d'un changement de niveau de preuve | validation |
| `Certificate` formel | `MathematicallyProven` seulement si décisif, ré-exécuté à l'identique, confirmé par une méthode indépendante, intègre | formel |
| `HumanApproval` | HMAC-SHA256, porté, expirant, émis par un humain administrateur | sécurité |
| `SandboxPolicy` | isolation appliquée par le noyau, échec fermé | exécution |

## 4. Protocole inter-processus

`adapters/PROTOCOL.md` (JSON-lines, version 1.0) : `manifest`, `execute`,
`cancel` ; réponses `manifest`, `result`, `error`, `progress`. Utilisé par le
pont Python, les moteurs scientifiques, le code évolué et les workers
distribués (en-tête d'authentification en plus).
