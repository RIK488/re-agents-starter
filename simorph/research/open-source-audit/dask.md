# Dask

**Catégorie :** Infrastructure  
**Décision :** écarter  

## 1. Dépôt officiel

https://github.com/dask/dask

## 2. Fonctionnalités pertinentes pour SIMORPH

Parallélisme Python (arrays, dataframes, graphes de tâches), distribué.

## 3. Licence et restrictions

BSD-3-Clause (PyPI `dask` 2026.8.0).

## 4. Activité de maintenance

Actif.

## 5. Dépendances

Python.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python.

## 7. Besoins CPU/GPU

CPU cluster.

## 8. Interfaces disponibles

Python.

## 9. Limites connues

Redondant avec Ray pour SIMORPH.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Non prioritaire.

## 12. Décision motivée

**Écarter (redondant avec Ray).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.