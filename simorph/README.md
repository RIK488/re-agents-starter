# SIMORPH

Infrastructure universelle de simulation, de recherche et d'amélioration des
intelligences artificielles. Workspace Rust modulaire + adaptateurs Python/FMI.

* Architecture cible intégrale : `docs/ARCHITECTURE.md`
* Exigences : `docs/REQUIREMENTS.md` — Critères d'acceptation : `docs/ACCEPTANCE_CRITERIA.md`
* **État réel (implémenté et testé / partiel / planifié / recherche)** : `docs/STATUS.md`
* Décisions techniques : `docs/adr/` — Sécurité : `docs/SECURITY_MODEL.md`
* Exigences de recherche (capacités non acquises) : `docs/RESEARCH_REQUIREMENTS.md`
* Audit open source (fiches à 12 champs) : `research/open-source-audit/`

## Démarrage

```bash
cd simorph
cargo test --workspace            # toute capacité déclarée est testée
cargo run -p simorph-cli -- --help
cargo run -p simorph-web          # API HTTP + interface (http://127.0.0.1:8080)
```

## Organisation

| Répertoire | Contenu |
|---|---|
| `crates/` | noyau Rust : contrats, unités, simulation, cosimulation, expérimentation, optimisation, incertitude, validation, provenance, registre, connaissance, ordonnanceur, exécution, orchestrateur, SDK plugin, sécurité, télémétrie |
| `services/` | directeur scientifique, fabriques de modèles et de simulateurs, laboratoire IA, évaluation, entraînement, service de connaissance |
| `plugins/` | moteurs de domaine (physique, ingénierie, IA) — aucun domaine n'est codé dans le noyau |
| `adapters/` | pont Python (protocole JSON-lines), FMI, outils externes |
| `apps/` | CLI, API web et interface |
| `schemas/` | schémas JSON exportés depuis les types Rust |
| `benchmarks/`, `tests/` | bancs de référence et tests d'intégration |
| `infrastructure/` | Dockerfile, docker-compose, Kubernetes, Prometheus |
| `docs/`, `research/` | documentation cible, audit open source, programmes de recherche |
