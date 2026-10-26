# Dagster / Prefect

**Catégorie :** Complémentaire : orchestration de données  
**Décision :** écarter  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/dagster-io/dagster ; https://github.com/PrefectHQ/prefect

## 2. Fonctionnalités pertinentes pour SIMORPH

Orchestration de pipelines de données Python avec assets, lignage, planification, UI.

## 3. Licence et restrictions

Apache-2.0 (PyPI `dagster` 1.13.27, `prefect` 3.9.0) ; offres cloud commerciales séparées.

## 4. Activité de maintenance

Très actifs.

## 5. Dépendances

Python, base de métadonnées.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Redondant avec simorph-scheduler + Temporal ; utile seulement pour pipelines de données d'entraînement.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Python, UI, GraphQL/REST.

## 9. Limites connues

Double orchestration.

## 10. Adaptateur SIMORPH nécessaire

HTTP.

## 11. Tests d'intégration prévus

Non prioritaire.

## 12. Décision motivée

**Écarter.** Le scheduler Rust et Temporal couvrent le besoin.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.