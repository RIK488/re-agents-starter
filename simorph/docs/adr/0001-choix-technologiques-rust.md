# ADR-0001 — Choix technologiques du noyau Rust

Statut : accepté (révisable sur mesure). Date : 2026-10-26.

## Contexte
Le cahier des charges (§4.3) impose d'évaluer Tokio, Rayon, Axum, Tonic, Serde,
PyO3, Arrow, Polars, Tracing, Wasmtime et de ne retenir chaque technologie
qu'après comparaison. Rust ne doit pas remplacer artificiellement les
bibliothèques scientifiques Python/Julia/C++/CUDA.

## Comparaison

| Besoin | Candidats | Décision | Motif |
|---|---|---|---|
| Exécution asynchrone (ordonnanceur, services) | Tokio, async-std, smol | **Tokio** | Écosystème (axum, tonic, tower), runtime multi-thread mature, annulation par `JoinHandle::abort`, `spawn_blocking` pour les composants synchrones. async-std est en maintenance réduite. |
| Parallélisme de calcul (plans d'expériences, Monte Carlo, Jacobi) | Rayon, std::thread, crossbeam | **Rayon** | Vol de travail, API itérateurs, déterminisme si les échantillons sont générés avant la parallélisation (règle appliquée dans `simorph-uncertainty`). |
| Services HTTP | Axum, Actix-web, Rocket | **Axum** | Intégré à Tokio/Tower, typage des extracteurs, middlewares (CORS, fichiers statiques). Actix est performant mais à runtime propre ; Rocket moins actif. |
| RPC inter-services | Tonic (gRPC), JSON/HTTP, Cap'n Proto | **JSON-lines pour les composants, HTTP/JSON pour l'API, Tonic en option** | Le contrat §7 doit être implémentable par un script Python en dix lignes : JSON-lines sur stdio l'emporte. gRPC réservé aux services distribués (Tonic, feature `grpc`, non activée tant qu'un besoin mesuré n'existe pas). |
| Sérialisation | Serde (+ serde_json), bincode, rkyv | **Serde + serde_json** | Lisible, schéma JSON via `schemars`, interopérable Python. Formats binaires (Arrow) réservés aux grands tableaux. |
| Interop Python | PyO3 (in-process), sous-processus JSON-lines, HTTP | **Sous-processus JSON-lines d'abord, PyO3 en option** | L'isolation (bac à sable, délais, plantages de bibliothèques natives, GIL) est une exigence de sécurité (§10.4). PyO3 sera ajouté pour les chemins où la copie de données domine (feature `pyo3`), après mesure. |
| Échange de données volumineuses | Arrow (arrow-rs), Parquet, CSV | **Arrow/Parquet pour trajectoires et ensembles > ~10 MB** (planifié) | Zéro-copie vers Polars/NumPy/DuckDB. JSON reste le format de contrôle. |
| Traitement analytique | Polars, DataFusion, DuckDB | **DuckDB (fichiers) + Polars (en mémoire)** (planifié) | Requêtes SQL sur archives d'expériences ; Polars pour agrégations dans le noyau. |
| Observabilité | tracing + tracing-subscriber, log, OpenTelemetry | **tracing** avec export OTLP (planifié) et exposition Prometheus (implémentée) | Spans structurés par run/experiment, compatible OpenTelemetry. |
| Composants isolés portables | Wasmtime (WASI), conteneurs, sous-processus | **Sous-processus + conteneurs ; Wasmtime planifié** pour les composants générés déterministes et sans E/S | Wasmtime donne un bac à sable fort et déterministe mais sans accès aux bibliothèques numériques natives. Adapté aux solveurs générés en Rust/C compilés en wasm32-wasi. |
| Algèbre linéaire | nalgebra, ndarray (+ BLAS), faer | **nalgebra** pour les petits systèmes (Newton, GP, surfaces de réponse) ; **faer/ndarray+BLAS** planifiés pour les grands | nalgebra est pure Rust, sans dépendance système ; les grands systèmes creux restent délégués à SUNDIALS/PETSc via adaptateurs. |

## Conséquences
* Le noyau est pur Rust, sans dépendance système, compilable sur poste local.
* Les bibliothèques scientifiques externes sont intégrées par adaptateurs
  (`adapters/`), avec un manifeste §7 produit par l'adaptateur.
* Chaque ajout de technologie (PyO3, Arrow, Tonic, Wasmtime) exige une mesure
  justifiant le gain, consignée dans un nouvel ADR.
