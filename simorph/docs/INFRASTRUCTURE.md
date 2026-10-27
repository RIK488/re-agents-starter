# SIMORPH — Architecture d'infrastructure et de déploiement

L'état réel de chaque élément est tenu dans `STATUS.md`. Ce document fixe la
cible et les choix ; il ne prétend pas que tout est déployé.

## 1. Profils de déploiement

| Profil | Usage | Composition | Stockage |
|---|---|---|---|
| **Poste** | développement, démonstrations | binaires `simorph`, `simorph-web`, `simorph-worker` ; `.venv` Python (`scripts/setup-python.sh`) | répertoire `--data` : JSONL, JSON, Markdown |
| **Station GPU** | entraînement, simulations lourdes | idem + workers déclarant leurs GPU, PyTorch CUDA | idem + disque local rapide pour jeux de données |
| **Cluster** | campagnes parallèles, multi-utilisateurs | `simorph-web` (2 réplicas) + coordinateur + workers (Deployment/Jobs Kubernetes), PostgreSQL, stockage objet | PostgreSQL (registre, provenance), S3/MinIO (artefacts), Qdrant (mémoire vectorielle) |
| **Isolé** (air-gapped) | données sensibles | images construites hors ligne, miroir PyPI/crates interne | identique au cluster sans accès externe |

## 2. Services et ports

| Service | Binaire | Rôle | Port |
|---|---|---|---|
| API + interface | `simorph-web` | actions, suivi des tâches, interface | 8080 |
| Coordinateur | `simorph-distributed` (dans le serveur ou autonome) | file de tâches, placement | 7070 |
| Worker | `simorph-worker` | exécution de composants | sortant uniquement |
| Métriques | `/metrics` de `simorph-web` | Prometheus | 8080 |
| Collecteur OTLP | externe | traces | 4317 |

## 3. Données

| Donnée | Format local | Format cluster | Intégrité |
|---|---|---|---|
| Provenance | `provenance.jsonl` chaîné | table PostgreSQL append-only + export JSONL | SHA-256 chaîné, vérifié au chargement |
| Expériences | `experiments/<id>.json` | stockage objet + index PostgreSQL | hachages du plan, des sorties, du composant |
| Jeux de données | JSON-lines haché | stockage objet, versionnés (DVC) | SHA-256 dans la provenance |
| Modèles entraînés | JSON (poids natifs) / fichiers PyTorch | stockage objet, suivi MLflow | hachage des poids |
| Suites scellées | JSON scellé | idem, accès en écriture Admin uniquement | sceau + approbation |
| Mémoire | `knowledge/knowledge.json` | PostgreSQL + Qdrant | lien vers enregistrements de provenance |

## 4. Isolation et sécurité opérationnelle

* Conteneurs non privilégiés, système de fichiers racine en lecture seule,
  `cap_drop: ALL`, profil seccomp par défaut (`infrastructure/`).
* Code généré : sous-processus avec limites système (CPU, mémoire, fichiers,
  processus), espace de noms réseau vide, répertoire jetable ; en cluster,
  Job Kubernetes dédié avec `NetworkPolicy` « deny all ».
* Secrets : variables d'environnement injectées (Secret Kubernetes), jamais
  en clair dans le dépôt ; masquage dans les journaux.
* Authentification : clé API aujourd'hui ; cible OIDC + RBAC SIMORPH sur
  chaque route (voir `GAP_ANALYSIS.md`).
* Actions externes (instruments, déploiements) : `ExternalSideEffect` +
  approbation humaine signée.

## 5. Observabilité

* Métriques Prometheus : runs, tâches par type, durée, échecs, utilisation des
  workers.
* Traces `tracing` structurées par campagne, expérience et run ; export OTLP
  planifié.
* Journal d'audit des décisions d'autorisation (JSON-lines).

## 6. Sauvegarde et reprise

* Provenance et archives : sauvegarde quotidienne, vérification de chaîne
  après restauration (`simorph provenance verify`).
* Ordonnanceur : `snapshot()/resume()` ; coordinateur distribué : tâches non
  acquittées réémises après redémarrage.

## 7. Coûts

* Chaque tâche déclare ses ressources (`ResourceRequirements`) ; le
  compromis précision/temps/coût (§8) est évalué avant lancement.
* Budget de campagne (`Budget`) : temps, nombre de runs, coût ; arrêt
  automatique et motif enregistré.

## 8. Artefacts fournis

| Fichier | Contenu | Vérifié |
|---|---|---|
| `infrastructure/docker/Dockerfile` | image API + CLI | voir audit (chemins, image de base) |
| `infrastructure/docker-compose.yml` | local + profils stockage/observabilité | non exécuté (pas de démon Docker ici) |
| `infrastructure/kubernetes/simorph-web.yaml` | Deployment, Service, PVC, Job | non appliqué (pas de cluster ici) |
| `scripts/setup-python.sh` | environnement scientifique Python | exécuté (SciPy, SymPy, Z3, MuJoCo, Gymnasium) |
