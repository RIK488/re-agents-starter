# Audit open source SIMORPH — synthèse (§6 du cahier des charges)

Date de l'audit : 2026-10-26. Nombre de fiches : 82 (63 projets listés au §6, 19 candidats complémentaires proposés par Claude, marqués *(compl.)*).

Chaque fiche suit exactement les 12 champs de l'« Audit obligatoire » du §6 et se termine par une ligne « Dernière vérification » qui distingue les faits vérifiés le jour de l'audit (métadonnées PyPI, crates.io, pages officielles, recherche web citée) des connaissances antérieures à revérifier sur le dépôt.

**Méthode de vérification.** L'API GitHub n'étant pas accessible depuis cette session, les licences ont été vérifiées en priorité via les métadonnées de paquets (`pypi.org/pypi/<pkg>/json`, `crates.io/api/v1/crates/<crate>`) qui reflètent le fichier LICENSE publié par les mainteneurs, puis par recherche web sur les pages officielles pour les points sensibles (changements de licence, scissions, renommages). Lorsque le champ licence d'un paquet était vide ou qu'aucune source primaire n'a été atteinte, la fiche indique « connaissance antérieure » ou « non vérifié ».

## Tableau de synthèse

| Projet | Catégorie | Licence | Décision | Adaptateur |
|---|---|---|---|---|
| [AI Scientist / AI Scientist V2](ai-scientist.md) | Recherche scientifique autonome | custom non-OSI (V1/V2) ; Apache-2.0 (V3) | écarter (référence) | Container + python-bridge |
| [R&D-Agent](rd-agent.md) | Recherche scientifique autonome | MIT | adapter | python-bridge + container pour l'exécution des expériences |
| [ShinkaEvolve](shinkaevolve.md) | Recherche scientifique autonome | Apache-2.0 (non vérifié) | adapter (sous réserve licence) | python-bridge en conteneur |
| [Darwin Gödel Machine](darwin-godel-machine.md) | Recherche scientifique autonome | Apache-2.0 | écarter (principes repris) | Container |
| [LangGraph](langgraph.md) | Agents et développement autonome | MIT | adapter (optionnel) | python-bridge : un graphe LangGraph exposé comme Component |
| [AutoGen](autogen.md) | Agents et développement autonome | MIT (AutoGen) / Apache-2.0 (AG2) | écarter / option AG2 | python-bridge |
| [CrewAI](crewai.md) | Agents et développement autonome | MIT | écarter | python-bridge |
| [OpenHands](openhands.md) | Agents et développement autonome | MIT | adapter (optionnel) | Container + HTTP |
| [SWE-agent](swe-agent.md) | Agents et développement autonome | MIT | écarter / option | Container + python-bridge |
| [Aider](aider.md) | Agents et développement autonome | Apache-2.0 | adapter (optionnel) | Container + subprocess |
| [NumPy](numpy.md) | Modélisation scientifique | BSD-3-Clause (+ composants permissifs) | intégrer | python-bridge |
| [SciPy](scipy.md) | Modélisation scientifique | BSD-3-Clause | intégrer | python-bridge |
| [SymPy](sympy.md) | Modélisation scientifique | BSD-3-Clause | intégrer | python-bridge |
| [JAX](jax.md) | Modélisation scientifique | Apache-2.0 | intégrer | python-bridge |
| [DifferentialEquations.jl](differentialequations-jl.md) | Modélisation scientifique | MIT | intégrer (phase 2) | Nouveau : adapters/julia |
| [OpenModelica](openmodelica.md) | Modélisation scientifique | OSMC-PL ou AGPL-3/GPL-3 | adapter (conteneur, FMI) | FMI + container pour omc |
| [OpenMDAO](openmdao.md) | Modélisation scientifique | Apache-2.0 | intégrer | python-bridge |
| [MuJoCo](mujoco.md) | Simulation physique et robotique | Apache-2.0 | intégrer | python-bridge |
| [NVIDIA Warp](nvidia-warp.md) | Simulation physique et robotique | Apache-2.0 (≥1.6) | intégrer (≥1.6) | python-bridge |
| [Brax](brax.md) | Simulation physique et robotique | Apache-2.0 | écarter (MJX) | python-bridge |
| [Project Chrono](project-chrono.md) | Simulation physique et robotique | BSD-3-Clause | adapter (phase 2) | FMI ou python-bridge |
| [Bullet / PyBullet](bullet.md) | Simulation physique et robotique | zlib | écarter | python-bridge |
| [Gazebo](gazebo.md) | Simulation physique et robotique | Apache-2.0 | adapter (phase 3) | Container + HTTP/gRPC |
| [ROS 2](ros2.md) | Simulation physique et robotique | Apache-2.0 (cœur) | adapter (phase 3) | HTTP/gRPC passerelle ou Rust rclrs |
| [Mesa](mesa.md) | Simulation multiagents | Apache-2.0 | intégrer | python-bridge |
| [Gymnasium](gymnasium.md) | Simulation multiagents / RL | MIT | intégrer | python-bridge |
| [PettingZoo](pettingzoo.md) | Simulation multiagents / RL | MIT | intégrer (optionnel) | python-bridge |
| [FMI](fmi-standard.md) | Cosimulation | BSD-2 (code) / CC-BY-SA 4.0 (docs) | intégrer | FMI |
| [FMPy](fmpy.md) | Cosimulation | BSD-3-Clause | intégrer | python-bridge |
| [preCICE](precice.md) | Cosimulation | LGPL-3.0 | adapter (phase 2) | in-process via FFI ou python-bridge |
| [Lean 4](lean4.md) | Vérification formelle | Apache-2.0 | intégrer (phase 2) | Subprocess dédié : adapters/external-tools/lean |
| [Z3](z3.md) | Vérification formelle | MIT | intégrer | in-process ou subprocess SMT-LIB |
| [Rocq Prover](rocq-coq.md) | Vérification formelle | LGPL-2.1 | adapter (optionnel) | Subprocess dédié |
| [Hypothesis](hypothesis.md) | Vérification formelle / tests | MPL-2.0 | intégrer (tests) | Aucun |
| [PyTorch](pytorch.md) | Entraînement des IA | BSD-3-Clause (+ EULA NVIDIA roues CUDA) | intégrer | python-bridge |
| [Transformers](transformers.md) | Entraînement des IA | Apache-2.0 (poids : licences propres) | intégrer | python-bridge |
| [PEFT](peft.md) | Entraînement des IA | Apache-2.0 | intégrer | python-bridge |
| [TRL](trl.md) | Entraînement des IA | Apache-2.0 | intégrer | python-bridge |
| [VERL](verl.md) | Entraînement des IA | Apache-2.0 | adapter (phase 3) | Container + python-bridge |
| [DeepSpeed](deepspeed.md) | Entraînement des IA | Apache-2.0 | intégrer (optionnel) | python-bridge/container |
| [Axolotl](axolotl.md) | Entraînement des IA | Apache-2.0 | écarter / option | Container |
| [Ray RLlib](ray-rllib.md) | Entraînement des IA | Apache-2.0 | adapter (phase 2) | python-bridge + Ray cluster |
| [Inspect AI](inspect-ai.md) | Évaluation | MIT | intégrer | python-bridge |
| [LM Evaluation Harness](lm-evaluation-harness.md) | Évaluation | MIT | intégrer | python-bridge/subprocess |
| [LightEval](lighteval.md) | Évaluation | MIT | écarter / contrôle croisé | subprocess |
| [HELM](helm.md) | Évaluation | Apache-2.0 | adapter (optionnel) | subprocess/container |
| [PostgreSQL](postgresql.md) | Données et expérimentation | PostgreSQL License | intégrer | in-process |
| [DuckDB](duckdb.md) | Données et expérimentation | MIT | intégrer | in-process |
| [Qdrant](qdrant.md) | Données et expérimentation | Apache-2.0 | intégrer | HTTP/gRPC |
| [MLflow](mlflow.md) | Données et expérimentation | Apache-2.0 | adapter (export) | HTTP |
| [DVC](dvc.md) | Données et expérimentation | Apache-2.0 | intégrer | subprocess |
| [Optuna](optuna.md) | Données et expérimentation | MIT | intégrer | python-bridge |
| [Ray](ray.md) | Infrastructure | Apache-2.0 | adapter (phase 2) | python-bridge + container |
| [Dask](dask.md) | Infrastructure | BSD-3-Clause | écarter | python-bridge |
| [Kubernetes](kubernetes.md) | Infrastructure | Apache-2.0 | intégrer | HTTP |
| [Temporal](temporal.md) | Infrastructure | MIT | adapter (phase 3) | gRPC |
| [OpenTelemetry](opentelemetry.md) | Infrastructure | Apache-2.0 | intégrer | in-process |
| [vLLM](vllm.md) | Infrastructure (inférence) | Apache-2.0 | intégrer | HTTP |
| [llama.cpp](llama-cpp.md) | Infrastructure (inférence) | MIT | intégrer | HTTP / in-process FFI |
| [SGLang](sglang.md) | Infrastructure (inférence) | Apache-2.0 | écarter / alternative | HTTP |
| [Three.js](threejs.md) | Visualisation | MIT | intégrer | Aucun |
| [Plotly](plotly.md) | Visualisation | MIT | intégrer | Aucun |
| [ParaView / VTK](paraview.md) | Visualisation | BSD-3-Clause | intégrer (phase 2) | python-bridge |
| [SUNDIALS](sundials.md) *(compl.)* | Complémentaire : solveurs numériques | BSD-3-Clause | intégrer (phase 2) | in-process FFI |
| [PETSc](petsc.md) *(compl.)* | Complémentaire : solveurs numériques | BSD-2-Clause | adapter (phase 3) | python-bridge |
| [FEniCSx](fenicsx.md) *(compl.)* | Complémentaire : éléments finis | LGPL-3.0-or-later | adapter (phase 2) | Container + python-bridge |
| [OpenFOAM](openfoam.md) *(compl.)* | Complémentaire : CFD | GPL-3.0-or-later | adapter (conteneur, phase 3) | Container + subprocess |
| [Kratos Multiphysics](kratos.md) *(compl.)* | Complémentaire : multiphysique FEM | BSD-4-Clause | adapter (optionnel) | python-bridge |
| [CasADi](casadi.md) *(compl.)* | Complémentaire : optimisation différentiable / commande | LGPL-3.0-or-later | intégrer (phase 2) | python-bridge |
| [Pyomo](pyomo.md) *(compl.)* | Complémentaire : optimisation mathématique | BSD-3-Clause | intégrer (phase 2) | python-bridge |
| [pymoo](pymoo.md) *(compl.)* | Complémentaire : optimisation multiobjectif | Apache-2.0 | intégrer (contrôle croisé) | python-bridge |
| [BoTorch / Ax](botorch-ax.md) *(compl.)* | Complémentaire : optimisation bayésienne | MIT | intégrer (phase 2) | python-bridge |
| [SALib](salib.md) *(compl.)* | Complémentaire : analyse de sensibilité | MIT | intégrer (contrôle croisé) | python-bridge |
| [UQpy / Chaospy](uqpy-chaospy.md) *(compl.)* | Complémentaire : quantification d'incertitude | MIT | adapter (phase 2) | python-bridge |
| [Nix / GNU Guix](nix-guix.md) *(compl.)* | Complémentaire : reproductibilité | LGPL-2.1 (Nix) / GPL-3 (Guix) | intégrer (infra) | Aucun |
| [Wasmtime](wasmtime.md) *(compl.)* | Complémentaire : exécution isolée | Apache-2.0 WITH LLVM-exception | intégrer (phase 2) | in-process |
| [Apache Arrow / Polars](arrow-polars.md) *(compl.)* | Complémentaire : données | Apache-2.0 / MIT | intégrer (phase 2) | in-process |
| [nalgebra / ndarray](nalgebra-ndarray.md) *(compl.)* | Complémentaire : algèbre linéaire Rust | Apache-2.0 / MIT OR Apache-2.0 | intégrer | in-process |
| [Crates Rust d'intégration d'EDO : diffsol, ode_solvers](rust-ode-crates.md) *(compl.)* | Complémentaire : solveurs Rust | MIT (diffsol) / Apache-2.0 | intégrer (phase 2) | in-process |
| [Dagster / Prefect](dagster-prefect.md) *(compl.)* | Complémentaire : orchestration de données | Apache-2.0 | écarter | HTTP |
| [Langfuse / Arize Phoenix](langfuse-phoenix.md) *(compl.)* | Complémentaire : observabilité IA | MIT sauf /ee (Langfuse) / ELv2 (Phoenix) | adapter Langfuse / écarter Phoenix | HTTP |
| [CodeCarbon](codecarbon.md) *(compl.)* | Complémentaire : coût énergétique | MIT | intégrer (phase 2) | python-bridge / in-process |

Légende des décisions : **intégrer** = dépendance ou outil retenu ; **adapter** = retenu via un adaptateur/conteneur, souvent en phase ultérieure ; **écarter** = non retenu (raison dans la fiche) ; mentions « (phase 2/3) » = calendrier de réalisation progressive, l'exigence cible restant entière (§1, règle de non-réduction).

## Compatibilité des licences avec un usage commercial

SIMORPH est distribué sous Apache-2.0 (workspace Rust). Conséquences par famille de licences rencontrées :

| Famille | Projets de l'audit | Implication pour une distribution SIMORPH |
|---|---|---|
| Permissives (MIT, BSD-2/3, Apache-2.0, zlib, PostgreSQL, Apache-2.0 WITH LLVM-exception) | NumPy, SciPy, SymPy, JAX, PyTorch, Transformers, PEFT, TRL, VERL, DeepSpeed, MuJoCo, Warp ≥ 1.6, Brax, Chrono, Bullet, Gazebo, ROS 2, Mesa, Gymnasium, PettingZoo, FMI (code), FMPy, Lean 4, Z3, Ray, Dask, Kubernetes, Temporal, OpenTelemetry, vLLM, llama.cpp, SGLang, Three.js, Plotly, ParaView, DuckDB, Qdrant, MLflow, DVC, Optuna, Inspect AI, lm-eval, LightEval, HELM, R&D-Agent, LangGraph, AG2, CrewAI, OpenHands, SWE-agent, Aider, DGM, SUNDIALS, PETSc, Kratos (BSD-4), Pyomo, pymoo, BoTorch/Ax, SALib, UQpy/Chaospy, Wasmtime, Arrow, Polars, nalgebra, ndarray, diffsol, Dagster, Prefect, CodeCarbon | Liaison statique ou dynamique, modification et redistribution libres, y compris commerciales, sous réserve de conserver les notices. BSD-4 (Kratos) ajoute une clause publicitaire incompatible avec la GPL : ne pas mélanger Kratos et du code GPL dans un même binaire. |
| Copyleft faible par fichier (MPL-2.0) | Hypothesis | Utilisable comme dépendance de test sans contrainte ; seules les modifications des fichiers MPL doivent être republiées. |
| Copyleft faible de bibliothèque (LGPL-2.1 / LGPL-3.0) | preCICE, FEniCSx (DOLFINx), CasADi, Rocq/Coq, Nix | Liaison **dynamique** acceptable depuis un logiciel propriétaire ou Apache ; liaison statique ou modification de la bibliothèque impose de publier la bibliothèque modifiée et de permettre la re-liaison. Règle SIMORPH : LGPL uniquement via FFI dynamique ou via le pont Python (processus séparé), jamais vendu dans un binaire Rust statique. |
| Copyleft fort (GPL-3.0, AGPL-3.0) | OpenFOAM (GPL-3), OpenModelica (AGPL-3/GPL-3 ou OSMC-PL réservé aux membres), Guix (GPL-3), Hydra (GPL-3) | Toute œuvre dérivée doit être GPL/AGPL. Règle SIMORPH : exécution dans un **processus/conteneur séparé** piloté par fichiers ou par le protocole JSON-lines, aucune liaison de code ; les FMU générés par OpenModelica embarquent du run-time dont la licence (option BSD pour certains fichiers) doit être vérifiée avant redistribution. L'AGPL ajoute l'obligation de publication pour un usage en service réseau. |
| Source-available non OSI | Arize Phoenix (Elastic License 2.0), AI Scientist V1/V2 (« AI Scientist Source Code License » dérivée de la Responsible AI License), NVIDIA Warp < 1.6 (NVIDIA Software License) | Pas open source au sens OSI : restrictions d'usage (ELv2 : interdiction de service managé ; RAIL : restrictions comportementales et obligation de divulgation). Écartés comme dépendances. |
| Licences de **poids de modèles** | Modèles chargés via Transformers/vLLM/llama.cpp (Llama, Gemma, Qwen, Mistral…) | Indépendantes de la licence de la bibliothèque ; chaque modèle a sa propre licence (souvent custom, parfois gated). Le registre de provenance SIMORPH doit enregistrer la licence de chaque poids utilisé. |
| Composants vendus sous EULA | Roues PyTorch CUDA (bibliothèques NVIDIA), solveurs liés à CasADi/Pyomo (SNOPT, Gurobi) | Redistribution soumise aux conditions NVIDIA/éditeur ; à exclure des images SIMORPH redistribuées, à installer côté utilisateur. |

Points de vigilance consignés dans les fiches : changement de licence de NVIDIA Warp (propriétaire → Apache-2.0 en 2025) ; scission AutoGen (MIT, maintenance) / AG2 (Apache-2.0) / Microsoft Agent Framework (MIT) ; renommage Coq → Rocq (LGPL-2.1 inchangée) ; OSMC-PL 1.8 passant de GPLv3 à AGPLv3 ; relicenciement de l'AI Scientist en décembre 2025 ; Langfuse MIT hors dossiers `ee`.

## Distinctions

Le cahier des charges impose de ne pas confondre quatre notions. L'audit en fournit des exemples concrets :

1. **Présence sur GitHub ≠ open source.** Arize Phoenix et AI Scientist V2 sont sur GitHub avec code lisible, mais sous des licences non OSI (ELv2, licence RAIL dérivée) qui restreignent l'usage. NVIDIA Warp était dans ce cas jusqu'à la version 1.6. Le dépôt `jennyzzt/dgm` est Apache-2.0 mais ses nombreuses réimplémentations homonymes ont d'autres licences : le nom ne garantit rien.
2. **Open source ≠ liberté d'usage commercial sans condition.** OpenFOAM et OpenModelica sont open source (GPL/AGPL) mais toute œuvre dérivée doit l'être aussi ; OpenModelica ne lève cette contrainte que pour les membres payants du consortium (OSMC-PL/EPL). Les bibliothèques LGPL (preCICE, CasADi, DOLFINx) sont utilisables commercialement, mais seulement en liaison dynamique. Kratos (BSD-4) est permissif mais incompatible avec la GPL.
3. **Liberté d'usage ≠ facilité d'intégration.** Gazebo, ROS 2, OpenFOAM, FEniCSx, PETSc sont permissifs ou LGPL mais lourds à construire et sans API stable adaptée à un noyau Rust : ils exigent conteneurs, passerelles et adaptateurs dédiés. Inversement Z3, DuckDB, Qdrant ou Wasmtime offrent une intégration in-process immédiate. L'AI Scientist et la DGM sont des scripts de recherche sans API : même avec une licence libre, ils ne seraient intégrables qu'en conteneur.
4. **Facilité d'intégration ≠ validité scientifique.** Un composant intégré ne devient pas « validé » : chaque fiche prévoit un test d'intégration contre une référence analytique ou publiée (Ishigami, Robertson, Sellar, cavité de Ghia, FMU de référence FMI), et ses sorties restent au niveau d'évidence `Simulated`/`Computed` tant que simorph-validation ne les a pas confrontées à une référence (§3.8).

Les paquets homonymes constituent un risque supplémentaire : `shinka` sur PyPI n'est pas ShinkaEvolve, `pyautogen`/`autogen` appartiennent à AG2 et non à Microsoft. Les manifestes de composants SIMORPH doivent donc référencer le dépôt et le hash de commit, pas seulement un nom de paquet.
