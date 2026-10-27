# SIMORPH — Plan de réalisation de l'architecture complète

Ce plan couvre les vingt sous-systèmes jusqu'à l'état final. Il remplace
`ROADMAP.md` (tranches T0–T7, conservées pour l'historique). Chaque jalon a
un critère de sortie **mesurable** ; un jalon n'est franchi que lorsque son
critère est démontré par un test ou un protocole archivé dans la provenance.

## 0. État de départ (fin de cette livraison)

Voir `STATUS.md` et `GAP_ANALYSIS.md`. En résumé : noyau, moteurs natifs,
moteurs Python matures (SciPy, SymPy, MuJoCo, Gymnasium, PyTorch CPU), formel
(Z3), évolution, apprentissage natif et RL, distribué TCP, multimodal,
connecteurs, isolation système, boucle d'amélioration démontrée sur des
réseaux de substitution. Manquent surtout : accès LLM, GPU/cluster, données
réelles, EDP, FMU exécutables, multi-utilisateur.

## 1. Phases

| Phase | Objet | Dépendances | Critère de sortie |
|---|---|---|---|
| **P1 Consolidation du noyau** | campagnes exécutées via l'ordonnanceur et le coordinateur distribué ; provenance à clé par défaut ; PostgreSQL pour registre et provenance | — | une campagne de 1 000 runs répartie sur 3 workers donne des hachages identiques à l'exécution locale ; chaîne vérifiée avec clé après redémarrage |
| **P2 Accès LLM gouverné** | serveur local (vLLM/llama.cpp) ou API ; raisonneur, interprète, mutateur d'évolution et critique branchés ; journal des appels ; budget | P1 | précision/rappel de la compréhension ≥ 0,95 sur 200 énoncés étiquetés ; aucun niveau de preuve accordé par un LLM (test) |
| **P3 Calcul scientifique étendu** | FMPy (FMU), OpenModelica en conteneur, FEniCSx/PETSc (EDP), Chrono, JAX/MJX | P1 | chaque moteur recoupé avec une référence publiée (benchmarks NAFEMS / Modelica Reference FMUs) |
| **P4 Entraînement GPU** | PyTorch CUDA, Transformers, PEFT/TRL, DeepSpeed ; workers GPU déclarés ; jobs isolés | P1 | fine-tuning LoRA d'un modèle ≥ 1 B paramètres reproductible (graine, hachages) ; débit mesuré |
| **P5 Évaluation de modèles** | Inspect AI, lm-evaluation-harness, suites scellées sous registre protégé, fiches de score | P4 | une évaluation de référence reproduite à ± 0,5 point des résultats publiés du harnais |
| **P6 Amélioration de modèles de langage** | boucle §5.1 appliquée à un LLM : limites → tâches générées et vérifiées (formel, simulation) → données validées → SFT/RL → comparaison scellée → acceptation/rejet | P2, P4, P5 | gain statistiquement significatif sur une suite scellée **hors distribution** des données d'entraînement, sans régression des métriques protégées, avec témoin négatif rejeté |
| **P7 Données et instruments réels** | connecteurs vers jeux de données publics et instruments réels ; calibration ; niveau « validé empiriquement » sur données mesurées | P1 | un modèle physique validé empiriquement sur des mesures publiées, avec couverture des incertitudes vérifiée |
| **P8 Multi-utilisateur et déploiement** | OIDC, RBAC par route, Kubernetes, stockage objet, OTLP, sauvegardes | P1 | test d'intrusion de base réussi ; restauration de sauvegarde vérifiée par la chaîne de provenance |
| **P9 Auto-amélioration de SIMORPH** | propositions de modifications de configuration puis de code évaluées en bac à sable par suites scellées, déploiement sous approbation humaine | P2, P5, P8 | au moins une amélioration mesurée d'un composant interne (ex. sélecteur de méthode) acceptée, et une modification dégradante rejetée ; aucune modification des juges possible (test) |
| **R Recherche** | RR-1 à RR-8 (`RESEARCH_REQUIREMENTS.md`) | variable | protocoles publiés dans `research/`, résultats négatifs inclus |

## 2. Par sous-système

| # | Prochaine étape | Phase |
|---|---|---|
| 1 | campagnes via ordonnanceur + coordinateur | P1 |
| 2 | LLM gouverné par rôle, budgets, journal | P2 |
| 3 | jeu d'évaluation étiqueté, interprète LLM revalidé par règles | P2 |
| 4 | Modelica/FMU, EDP, découverte de lois multi-variables avec SymPy | P3 |
| 5 | génération par composition + solutions manufacturées pour la vérification | P3, RR-1 |
| 6 | FMPy, preCICE, extrapolation d'entrées, Newton sur boucles | P3 |
| 7 | modèles de vision/audio via PyTorch, maillages volumiques | P4 |
| 8 | FEniCSx, PETSc, Chrono, Warp | P3, P4 |
| 9 | valeur d'information par surrogates bayésiens ; contradictions résolues par réplication et haute fidélité | P1, RR-6 |
| 10 | mutateurs LLM, îles distribuées sur workers | P2, P1 |
| 11 | PyTorch CUDA, PEFT/TRL, DeepSpeed | P4 |
| 12 | RLlib, environnements MuJoCo complexes | P4 |
| 13 | Lean 4 + Mathlib, arithmétique d'intervalles validée sur trajectoires | R (RR-3) |
| 14 | Inspect AI, lm-eval, registre de suites protégé partout | P5 |
| 15 | PostgreSQL, Qdrant, MLflow, DVC ; analogies automatiques | P1, P7 |
| 16 | GPU, Ray, Jobs Kubernetes, TLS | P1, P4, P8 |
| 17 | montages en lecture seule, cgroups `pids.max`, conteneurs par run | P8 |
| 18 | OIDC, multi-utilisateur, ParaView | P8 |
| 19 | instruments réels, réglages série, flux temps réel | P7 |
| 20 | LLM (P6), puis SIMORPH lui-même (P9) | P6, P9 |

## 3. Règles de conduite du plan

* Aucune phase ne réduit la cible ; une phase bloquée par le matériel ou
  l'accès reste ouverte et est signalée, jamais déclarée achevée.
* Chaque capacité nouvelle arrive avec : un test contre une référence
  indépendante, un témoin négatif lorsque c'est pertinent, une mesure de
  performance et ses limites dans `STATUS.md`.
* Les juges (validation, suites scellées, critères, autorisations) sont
  modifiés uniquement par revue humaine.
