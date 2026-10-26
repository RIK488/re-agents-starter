# Infrastructure SIMORPH

| Configuration | Fichiers | Remarques |
|---|---|---|
| Locale (poste) | `cargo run -p simorph-cli`, `cargo run -p simorph-web` | Aucune dépendance système ; Python 3 + NumPy pour le pont Python. |
| Locale conteneurisée | `docker/Dockerfile`, `docker-compose.yml` | Profils `storage` (PostgreSQL, Qdrant) et `observability` (Prometheus, collecteur OpenTelemetry). |
| Cloud / cluster | `kubernetes/simorph-web.yaml` | Déploiement non privilégié, système de fichiers en lecture seule, secrets injectés, exemple de Job isolé pour un run. |

L'exécution distribuée multi-nœuds (Ray / Jobs Kubernetes pilotés par
l'ordonnanceur) est une intégration planifiée : voir `docs/STATUS.md`
(REQ-RUN-8).
