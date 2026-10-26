# SUNDIALS — LLNL

**Catégorie :** Complémentaire : solveurs numériques  
**Décision :** intégrer (phase 2)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/LLNL/sundials

## 2. Fonctionnalités pertinentes pour SIMORPH

CVODE/CVODES (ODE raides/non raides + sensibilités), IDA/IDAS (DAE), ARKODE (Runge-Kutta additifs/IMEX), KINSOL (non linéaire) ; référence industrielle pour les systèmes raides et l'analyse de sensibilité adjointe.

## 3. Licence et restrictions

BSD-3-Clause (LLNL) ; attention aux dépendances optionnelles (KLU LGPL, PETSc BSD-2, hypre Apache/MIT).

## 4. Activité de maintenance

Actif (7.5.0 septembre 2025).

## 5. Dépendances

C ; MPI/CUDA/HIP optionnels.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

FFI C depuis Rust (crate `sundials-sys` communautaire) in-process ; scikits-odes côté Python.

## 7. Besoins CPU/GPU

CPU ; GPU optionnel.

## 8. Interfaces disponibles

C, Fortran, Python (tiers).

## 9. Limites connues

API C verbeuse ; build CMake.

## 10. Adaptateur SIMORPH nécessaire

in-process FFI (phase 2).

## 11. Tests d'intégration prévus

Robertson et HIRES : CVODE BDF vs backward Euler/trapèze de simorph-simulation ; sensibilités CVODES vs Sobol de simorph-uncertainty sur un cas linéaire.

## 12. Décision motivée

**Intégrer (phase 2).** Solveur raide de référence pour remplacer/compléter les intégrateurs implicites Rust maison sur les gros systèmes.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.