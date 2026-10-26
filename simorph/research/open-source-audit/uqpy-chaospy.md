# UQpy / Chaospy

**Catégorie :** Complémentaire : quantification d'incertitude  
**Décision :** adapter (phase 2)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/SURGroup/UQpy ; https://github.com/jonathf/chaospy

## 2. Fonctionnalités pertinentes pour SIMORPH

Chaos polynomial, krigeage, fiabilité (FORM/SORM, subset simulation), échantillonnage avancé (MCMC, importance), distributions copules.

## 3. Licence et restrictions

MIT (PyPI `UQpy` 4.2.1, `chaospy` 4.3.21).

## 4. Activité de maintenance

Modéré (releases 2025).

## 5. Dépendances

NumPy, SciPy, scikit-learn.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Python.

## 9. Limites connues

Maintenance moins intense.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

PCE d'ordre 3 sur Ishigami : moments vs analytique ; FORM sur un cas de fiabilité à probabilité connue.

## 12. Décision motivée

**Adapter (phase 2).** Chaos polynomial et fiabilité au-delà de la surface de réponse Rust.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.