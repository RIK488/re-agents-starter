# SIMORPH — Réalisation progressive (sans réduction de la cible)

L'architecture cible, les exigences et les critères d'acceptation couvrent le
système final dès le départ. La réalisation est découpée en tranches
**techniques**, pas en réductions de périmètre. Chaque tranche livre des
composants testés ; rien n'est déclaré opérationnel sans test.

| Tranche | Contenu | Critères |
|---|---|---|
| T0 Fondation | Contrats (`simorph-core`), unités, documentation cible, audit open source | AC-CORE-*, AC-UNITS-* |
| T1 Moteurs natifs | Simulation, cosimulation, incertitude, optimisation, expérimentation, validation, provenance, registre, connaissance | AC-SIM, AC-COS, AC-UNC, AC-OPT, AC-EXP, AC-VAL, AC-PROV, AC-KNW |
| T2 Noyau d'exécution | Ordonnanceur, exécution, orchestrateur, sécurité, télémétrie, pont Python, FMI | AC-RUN, AC-SEC |
| T3 Services | Directeur scientifique, fabriques de modèles et de simulateurs, laboratoire IA, évaluation, entraînement, service de connaissance | AC-DIR, AC-AIL, AC-GEN |
| T4 Interface | CLI complet, API HTTP, UI web (espaces §12), visualisation 2D/3D, export | AC-UI |
| T5 Intégrations externes | Adaptateurs vers les projets retenus dans l'audit (SUNDIALS/SciPy, MuJoCo, FMPy, Lean/Z3, PyTorch/TRL, Inspect AI, MLflow, Ray, OpenTelemetry…) | tests d'intégration par fiche d'audit |
| T6 Distribué / cloud | Ray/Kubernetes, stockage objet, PostgreSQL/DuckDB/Qdrant, OTLP, secrets, coûts | REQ-RUN-8, §10.3 |
| T7 Recherche | Programmes RR-1…RR-8 | protocoles publiés dans `research/` |

Les tranches T1–T2 peuvent s'exécuter en parallèle ; T3 dépend de T1–T2 ;
T4 expose uniquement ce qui est testé.
