# MLflow

**Catégorie :** Données et expérimentation  
**Décision :** adapter (export)  

## 1. Dépôt officiel

https://github.com/mlflow/mlflow

## 2. Fonctionnalités pertinentes pour SIMORPH

Suivi d'expériences, registre de modèles, artefacts, évaluation LLM, traçage.

## 3. Licence et restrictions

Apache-2.0 (classifieur PyPI `mlflow` 3.17.2).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Python, SQLAlchemy, serveur optionnel.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

API REST depuis Rust ; Python natif.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

REST, Python, UI.

## 9. Limites connues

Modèle de données orienté ML ; moins riche que la provenance SIMORPH (hash-chain).

## 10. Adaptateur SIMORPH nécessaire

HTTP (export de la provenance vers MLflow).

## 11. Tests d'intégration prévus

Export d'une ExperimentRecord vers MLflow et relecture.

## 12. Décision motivée

**Adapter (export).** Pas de remplacement de simorph-provenance.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.