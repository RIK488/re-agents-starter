# Pyomo — Sandia

**Catégorie :** Complémentaire : optimisation mathématique  
**Décision :** intégrer (phase 2)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/Pyomo/pyomo

## 2. Fonctionnalités pertinentes pour SIMORPH

Modélisation algébrique (LP/MILP/NLP/MINLP, stochastique, DAE via pyomo.dae), interfaces vers de nombreux solveurs (HiGHS, CBC, Gurobi, IPOPT).

## 3. Licence et restrictions

BSD-3-Clause (PyPI `pyomo` 6.10.1).

## 4. Activité de maintenance

Actif.

## 5. Dépendances

Solveurs externes (HiGHS MIT, IPOPT EPL…).

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Python.

## 9. Limites connues

Dépend des solveurs ; MINLP difficile.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Knapsack résolu par HiGHS vs MCTS/exhaustif de simorph-optimization ; dimensionnement d'une batterie (LP) vs solution analytique.

## 12. Décision motivée

**Intégrer (phase 2).** Problèmes discrets/mixtes exacts.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.