# SIMORPH — Stratégie d'exécution et d'entraînement CPU/GPU

## 1. Principe

Le noyau Rust décide **où** et **comment** une tâche s'exécute à partir du
manifeste du composant (`ResourceRequirements` : cœurs, mémoire, GPU, durée)
et de l'inventaire réel des machines. Il n'invente jamais un GPU : un worker
déclare 0 GPU si `nvidia-smi`/`rocm-smi` ne répondent pas, et une tâche qui
exige un GPU sans worker éligible est déclarée non planifiable.

## 2. Niveaux de parallélisme

| Niveau | Mécanisme | Usage |
|---|---|---|
| Intra-tâche CPU | Rayon (vol de travail) | Monte Carlo, plans d'expériences, Jacobi, évaluations de population |
| Intra-tâche natif externe | BLAS/LAPACK, OpenMP des bibliothèques (SciPy, MuJoCo) | solveurs raides, algèbre linéaire |
| Inter-tâches local | `simorph-scheduler` (Tokio, admission par ressources) | graphes de runs |
| Inter-machines | `simorph-distributed` (coordinateur/workers TCP) | campagnes, évolution, ensembles |
| Cluster géré | Jobs Kubernetes ou Ray (adaptateur) | passage à l'échelle, GPU partagés |

Règle de déterminisme : les échantillons sont générés **avant** la
parallélisation et les résultats sont collectés dans l'ordre du plan, de
sorte que le nombre de threads ou de workers ne change pas les résultats.

## 3. Placement

1. Filtrer les workers qui exposent le composant demandé.
2. Filtrer par ressources libres (cœurs, mémoire, GPU, mémoire GPU).
3. Choisir par score : localité des données, charge, priorité de la tâche.
4. Délai par tâche ; sur perte de battement de cœur, réémission sur un autre
   worker ; déduplication par hachage de requête.

## 4. Entraînement

| Taille de modèle | Moteur | Matériel | Remarques |
|---|---|---|---|
| Petits réseaux (surrogates, politiques RL) | `simorph-learning` natif | CPU | déterministe, sans dépendance, poids hachés |
| Réseaux moyens | PyTorch via pont Python | CPU ou 1 GPU | même architecture rechargeable en natif pour l'évaluation |
| Modèles de langage (SFT, LoRA, RL) | PyTorch + Transformers + PEFT/TRL | 1–8 GPU | gabarits de `training-service`, jobs isolés |
| Grands modèles distribués | DeepSpeed / FSDP, VERL pour RL | multi-nœuds GPU | via Jobs Kubernetes ou Ray |

Contrôles imposés à tout entraînement déclenché par la boucle
d'amélioration (sous-système 20) :

* jeu de données validé et haché, provenance complète ;
* budget fixé (époques, temps, coût) et graine ;
* évaluation avant/après sur suite scellée, test statistique, métriques
  protégées sans régression ;
* acceptation ou rejet automatique, retour arrière possible ;
* aucun accès en écriture aux suites et aux critères.

## 5. Inférence

* Modèles natifs : composants in-process.
* Modèles PyTorch : composant Python persistant (processus chaud).
* LLM : serveurs vLLM / SGLang / llama.cpp derrière un client HTTP
  configurable ; utilisés par le raisonnement et les mutateurs d'évolution,
  jamais comme juges des niveaux de preuve.

## 6. Mesure

Chaque tâche publie `wall_time_ms` et ses métriques propres ; le
coordinateur publie débit et utilisation par worker. Les bancs
(`simorph bench`) consignent l'environnement matériel avec les résultats.

## 7. Environnement de cette livraison

Mesuré le 2026-10-27 : 4 cœurs, 15 Go de mémoire, aucun GPU, pas de démon
Docker, pas de cluster. Les chemins GPU et cluster sont donc décrits et
détectés, mais non exercés ; voir `STATUS.md`.
