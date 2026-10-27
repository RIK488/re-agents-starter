# SIMORPH — Matrice d'intégration des dépôts open source

Faits vérifiés le 2026-10-27 (licence, dernière version, installabilité) :
`audit/oss-facts-2026-10-27.md`. Fiches détaillées à 12 champs :
`../research/open-source-audit/`.

« Intégré (mesuré) » signifie : la bibliothèque est réellement exécutée par
SIMORPH dans cet environnement, derrière le contrat `Component`, avec des tests
qui comparent ses résultats à une référence indépendante. « Prévu » signifie
que l'adaptateur et la décision existent, pas l'exécution.

## 1. Décisions et état

| Projet | Licence | Version vérifiée | Rôle dans SIMORPH (sous-système) | Décision | État d'intégration | Preuve |
|---|---|---|---|---|---|---|
| SciPy | BSD-3-Clause | 1.18.1 | 8 : EDO raides (Radau, BDF, LSODA), optimisation (L-BFGS-B, SLSQP, HiGHS), QMC | intégrer | **Intégré (mesuré)** | `adapters/scientific-python` : Robertson à 6·10⁻¹², Van der Pol μ = 1000, LP Wyndor exact |
| SymPy | BSD-3-Clause | 1.14.0 | 4, 13 : solutions symboliques, vérification d'EDO, équivalences, jacobiennes | intégrer | **Intégré (mesuré)** | `dsolve` vérifié par `checkodesol` ; `simorph-formal` (équivalence de lois, jacobienne Van der Pol à 1,6·10⁻¹¹) |
| Z3 | MIT | 5.3.2 | 13 : preuves SMT certifiées, faisabilité de contraintes | intégrer | **Intégré (mesuré)** | `simorph-formal` : 10 cas prouvés/réfutés, re-vérifiés par intervalles, LP ou énumération |
| MuJoCo | Apache-2.0 | 3.17.0 | 8, 12 : dynamique multicorps, contacts | intégrer | **Intégré (mesuré)** | pendule à θ₀ = 2,5 rad à 2·10⁻⁷ de l'intégrale elliptique ; 40 k–140 k pas/s |
| Gymnasium | MIT | 1.4.0 | 12 : environnements RL de référence | intégrer | **Intégré (mesuré)** | CartPole-v1 recoupé avec l'implémentation native à 2,2·10⁻⁸ |
| PyTorch | BSD-3-Clause (+ EULA NVIDIA pour les roues CUDA) | 2.14.1 | 11 : entraînement | intégrer | **Intégré (mesuré, CPU)** | MLP régression déterministe (même empreinte de poids sur deux exécutions) ; pas de GPU ici |
| NumPy | BSD-3-Clause | 2.5 | support | intégrer | **Intégré** | pont Python |
| FMI 2.0/3.0 | BSD-2 (code) | 3.0.2 | 6 : FMU | intégrer | **Partiel** : lecture de `modelDescription.xml` ; exécution via FMPy non installée | `adapters/fmi` |
| JAX | Apache-2.0 | 0.12.0.1 | 8, 11 : simulation différentiable, MJX | intégrer | Prévu (installable, 90 Mo) | — |
| Transformers | Apache-2.0 (poids : licences propres) | 5.21.0 | 11 : modèles préentraînés | intégrer | Prévu ; gabarits dans `training-service` | — |
| TRL | Apache-2.0 | 1.16.0 | 11, 20 : SFT, DPO, GRPO | intégrer | Prévu ; GPU requis en pratique | — |
| DeepSpeed | Apache-2.0 | 0.19.8 | 11, 16 : entraînement distribué | intégrer (GPU) | Prévu ; compilation à l'installation, GPU | — |
| Ray RLlib | Apache-2.0 | 2.59.0 | 12, 16 : RL distribué | adapter | Prévu ; correspondance documentée dans `COMPUTE_STRATEGY.md` | — |
| OpenMDAO | Apache-2.0 | 3.45.1 | 9 : optimisation multidisciplinaire à dérivées | intégrer | Prévu (installable) | — |
| OpenModelica | AGPL-3.0 ou OSMC-PL 1.8 | 1.27.1 | 4, 6 : Modelica → FMU | adapter, processus isolé (copyleft) | Bloqué ici : `build.openmodelica.org` inaccessible (403) | — |
| Project Chrono | BSD-3-Clause | 10.0.0 | 8 : multicorps, éléments finis, véhicules | adapter | Prévu ; distribution conda uniquement (conda absent) | — |
| NVIDIA Warp | Apache-2.0 (≥ 1.6) | 1.18.0 | 8 : noyaux GPU différentiables | intégrer | Prévu ; utile seulement avec GPU | — |
| Lean 4 | Apache-2.0 | 4.35.0 | 13 : preuves interactives | intégrer | **Détecté absent** ; back-end prêt, renvoie `NotImplemented` avec la cause | `simorph-formal::lean` |
| Inspect AI | MIT | 0.3.285 | 14 : évaluation de modèles et d'agents | intégrer | Lanceur et analyseur testés sur un harnais factice ; bibliothèque non installée | `evaluation-service::harness` |
| lm-evaluation-harness | MIT | 0.4.13 | 14 : bancs LLM | intégrer | Idem | idem |
| MLflow | Apache-2.0 | 3.17.2 | 15 : suivi d'expériences, registre de modèles | intégrer | Prévu ; la provenance native couvre le suivi local | — |
| DVC | Apache-2.0 | 3.67.1 | 15 : versionnement de données | intégrer | Prévu ; jeux de données hachés nativement | — |
| PostgreSQL | PostgreSQL License | 16 (apt) / 18.6 | 15 : registre multi-utilisateur | intégrer (cluster) | Prévu ; JSONL vérifié en local | — |
| Kubernetes | Apache-2.0 | 1.37.1 | 16 : Jobs, GPU, isolation | intégrer (cluster) | Manifestes fournis, non appliqués (pas de cluster) | `infrastructure/kubernetes` |
| LangGraph | MIT | 1.2.18 | 2 : graphes d'agents Python | adapter (optionnel) | Non retenu pour le cœur : orchestrateur multi-agents natif en Rust | — |
| OpenHands | MIT | 1.11.0 | 17 : agent de développement en bac à sable | adapter (optionnel, conteneur) | Prévu ; exige Docker | — |
| R&D-Agent | MIT | 1.0.0 | 9, 20 : boucles R&D pilotées par données | adapter (conteneur) | Prévu ; Linux + Docker | — |
| ShinkaEvolve | Apache-2.0 | 0.0.7 (`shinka-evolve`) | 10 : évolution de programmes par LLM | reprendre les principes ; adapter en option | Moteur d'évolution natif `simorph-evolution` ; ShinkaEvolve non intégré (0.x, LLM requis) | — |
| Darwin Gödel Machine | Apache-2.0 | aucune version | 10, 20 : agent auto-modifiant | écarter comme dépendance, reprendre les principes | Non intégré ; garde-fous SIMORPH (évaluateur scellé, `SelfModify` sous approbation) | — |
| AI Scientist V2 | licence maison non OSI (RAIL v1.0) | aucune version | 9 : recherche autonome | écarter comme dépendance | Non intégré (licence, GPU, exécution de code LLM) | — |

## 2. Règles d'intégration

1. Une bibliothèque mature n'est pas réécrite ; elle est appelée derrière le
   contrat `Component`, en processus isolé (réseau coupé, mémoire et CPU
   limités), et ses résultats sont recoupés avec une référence indépendante.
2. Les moteurs natifs Rust sont conservés lorsqu'ils apportent un avantage
   mesuré (déterminisme, absence de dépendance, débit, intégration aux unités
   et à la provenance) ; ils sont alors recoupés avec la bibliothèque mature.
3. Licences copyleft (AGPL, GPL, LGPL) : processus séparé ou conteneur,
   jamais liées au binaire.
4. Noms de paquets ambigus (`pychrono`, `ai-scientist`, `lean4`, `shinka`) :
   installation depuis la source officielle uniquement.
5. Tout projet qui exécute du code généré par LLM (DGM, AI Scientist,
   R&D-Agent, OpenHands) n'est exécuté qu'en conteneur sans réseau.
