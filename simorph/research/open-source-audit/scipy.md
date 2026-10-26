# SciPy

**Catégorie :** Modélisation scientifique  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/scipy/scipy

## 2. Fonctionnalités pertinentes pour SIMORPH

Intégration ODE (solve_ivp : RK45, DOP853, Radau, BDF, LSODA), optimisation (minimize, differential_evolution, NSGA absent), interpolation, statistiques, algèbre creuse, traitement du signal.

## 3. Licence et restrictions

BSD-3-Clause (classifieur « BSD License » sur PyPI `scipy` 1.18.1).

## 4. Activité de maintenance

Très actif (1.18.1, 2026-08-21).

## 5. Dépendances

NumPy, compilateurs Fortran/C pour build.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; les intégrateurs implicites (Radau/BDF) complètent ceux de simorph-simulation pour les systèmes raides.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python.

## 9. Limites connues

Pas de GPU ; pas d'événements avancés dans solve_ivp (seulement fonctions d'événement scalaires).

## 10. Adaptateur SIMORPH nécessaire

python-bridge (exemple scipy_ode_component.py avec repli NumPy).

## 11. Tests d'intégration prévus

Robertson (raide) par Radau vs solution de référence publiée ; oscillateur harmonique vs analytique ; comparaison croisée avec simorph-simulation (RK45 Rust vs RK45 SciPy, écart < 1e-6 relatif).

## 12. Décision motivée

**Intégrer.** Référence de validation croisée pour simorph-simulation et fournisseur de solveurs raides côté Python.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.