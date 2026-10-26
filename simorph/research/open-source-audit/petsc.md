# PETSc (+ petsc4py)

**Catégorie :** Complémentaire : solveurs numériques  
**Décision :** adapter (phase 3)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://gitlab.com/petsc/petsc

## 2. Fonctionnalités pertinentes pour SIMORPH

Algèbre linéaire/non linéaire creuse parallèle (KSP, SNES), intégrateurs TS, DMPlex maillages ; base des codes PDE HPC.

## 3. Licence et restrictions

BSD-2-Clause (PyPI `petsc` et `petsc4py` 3.26.0, 2026-09-29).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

MPI, BLAS/LAPACK.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

FFI C ou petsc4py via pont Python.

## 7. Besoins CPU/GPU

CPU cluster ; GPU (CUDA/Kokkos).

## 8. Interfaces disponibles

C, Fortran, Python.

## 9. Limites connues

Complexité de build/configuration.

## 10. Adaptateur SIMORPH nécessaire

python-bridge (petsc4py) ; FFI phase 3.

## 11. Tests d'intégration prévus

Poisson 2D : convergence d'ordre 2 mesurée par simorph-validation (Richardson).

## 12. Décision motivée

**Adapter (phase 3, PDE/HPC).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.