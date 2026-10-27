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

## Exemples

```bash
simorph actions                          # toutes les actions (identiques à l'API HTTP)
simorph problem example nanosat > p.json # problème d'exemple (aérospatial)
simorph analyse p.json                   # connu / inconnu / calculable / à modéliser / à simuler / données / preuve / vérification
simorph plan p.json                      # hypothèses réfutables + programme expérimental (critères figés)
simorph campaign run p.json              # campagne complète : référence, criblage, optimisation, vérification, conclusions
simorph demo pipe                        # autre secteur (hydraulique minière) ; aussi beam, finance
simorph run physics.orbit.two_body -i mu=3.986004418e14 -i x=6878137 -i y=0 -i z=0 -i vx=0 -i vy=7612.6 -i vz=0 -i j2=0.00108263 -i r_body=6378137 -i t_end=5700
simorph uncertainty sobol engineering.beam.cantilever -i length=2 -i load=1000 -i youngs_modulus=2.1e11 -i second_moment=8.33e-6 --vary length=0.1 --vary load=100 --output tip_deflection
simorph lab run --suite reasoner         # boucle d'amélioration d'une IA sur une suite scellée
simorph simulator generate req.json      # pipeline de génération de simulateur en 10 étapes
simorph provenance verify                # intégrité de la chaîne de provenance
simorph knowledge memory                 # mémoire scientifique (§11)
```

Chaque campagne écrit `campaigns/<id>/report.md` et `campaign.json` dans le
répertoire de données (`--data`, défaut `.simorph`), et alimente la mémoire
scientifique du même répertoire.

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
