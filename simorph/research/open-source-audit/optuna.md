# Optuna

**Catégorie :** Données et expérimentation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/optuna/optuna

## 2. Fonctionnalités pertinentes pour SIMORPH

Optimisation d'hyperparamètres (TPE, CMA-ES, NSGA-II, Gaussian process), pruning, stockage, tableau de bord.

## 3. Licence et restrictions

MIT (classifieur PyPI `optuna` 5.0.0).

## 4. Activité de maintenance

Très actif (5.0 en 2026).

## 5. Dépendances

Python, SQLAlchemy optionnel.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; contrôle croisé de simorph-optimization.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Python, CLI, dashboard.

## 9. Limites connues

Python only.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Rosenbrock 2D : TPE Optuna vs CMA-ES Rust avec même budget ; les deux < 1e-3.

## 12. Décision motivée

**Intégrer.** Optimiseur Python de référence et oracle de validation croisée.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.