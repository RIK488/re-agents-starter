# OpenMDAO — NASA

**Catégorie :** Modélisation scientifique  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/OpenMDAO/OpenMDAO

## 2. Fonctionnalités pertinentes pour SIMORPH

Optimisation multidisciplinaire (MDO) : couplage de composants, résolution Newton/Gauss-Seidel des boucles, dérivées analytiques par graphe (MAUD), pilotes d'optimisation (SLSQP, pyOptSparse), enregistreurs.

## 3. Licence et restrictions

Apache-2.0 (PyPI `openmdao` 3.45.1, 2026-09-11).

## 4. Activité de maintenance

Très actif (NASA).

## 5. Dépendances

NumPy, SciPy, networkx ; pyOptSparse optionnel.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; son modèle de « composants couplés » est proche de simorph-cosimulation (Gauss-Seidel itératif) — validation croisée naturelle.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python, enregistrement SQLite, N2 diagram (HTML).

## 9. Limites connues

Statique (pas de cosimulation temporelle), Python only.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Problème Sellar (référence MDO) résolu par OpenMDAO et par simorph-cosimulation (GS itératif) : même point fixe à 1e-6.

## 12. Décision motivée

**Intégrer.** Référence et moteur MDO pour l'espace Optimisation ; cas Sellar comme test de non-régression de la cosimulation.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.