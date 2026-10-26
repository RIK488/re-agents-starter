# BoTorch / Ax — Meta

**Catégorie :** Complémentaire : optimisation bayésienne  
**Décision :** intégrer (phase 2)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/pytorch/botorch ; https://github.com/facebook/Ax

## 2. Fonctionnalités pertinentes pour SIMORPH

Optimisation bayésienne sur PyTorch (GP, acquisitions Monte Carlo, multi-objectif qEHVI, multi-fidélité qMFKG — directement utile pour §8), Ax pour l'orchestration d'expériences adaptatives.

## 3. Licence et restrictions

MIT (PyPI `botorch` 0.18.1, `ax-platform` 1.3.1).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

PyTorch, GPyTorch.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python.

## 7. Besoins CPU/GPU

CPU ; GPU optionnel.

## 8. Interfaces disponibles

Python.

## 9. Limites connues

Lourd pour de petits problèmes.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Branin : BoTorch qEI vs BO Rust (regret final comparable) ; multi-fidélité sur une fonction synthétique à deux niveaux de coût, économie mesurée.

## 12. Décision motivée

**Intégrer (phase 2).** BO multi-fidélité et multi-objectif, cœur de la simulation guidée par l'incertitude.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.