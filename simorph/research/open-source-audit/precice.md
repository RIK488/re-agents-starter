# preCICE

**Catégorie :** Cosimulation  
**Décision :** adapter (phase 2)  

## 1. Dépôt officiel

https://github.com/precice/precice

## 2. Fonctionnalités pertinentes pour SIMORPH

Bibliothèque de couplage multiphysique partitionné (FSI, CHT…) : mappage de données entre maillages, schémas de couplage implicites (quasi-Newton), communication MPI/sockets ; adaptateurs OpenFOAM, CalculiX, FEniCS, deal.II, SU2.

## 3. Licence et restrictions

LGPL-3.0 (LGPL-3.0-or-later selon les emballages) ; les auteurs indiquent explicitement l'usage avec des solveurs propriétaires.

## 4. Activité de maintenance

Actif (v3.x, 2026 ; pyprecice 3.4.0 2026-04-08).

## 5. Dépendances

C++, Boost, Eigen, MPI, PETSc optionnel, libxml2.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

LGPL : liaison dynamique acceptable depuis Rust (FFI C) sans contaminer le noyau ; API Python via pont. Couvre le couplage de champs (maillages), complémentaire au couplage scalaire de simorph-cosimulation.

## 7. Besoins CPU/GPU

CPU (HPC/MPI).

## 8. Interfaces disponibles

C++, C, Fortran, Python, Julia, Rust (binding communautaire).

## 9. Limites connues

Orienté champs/maillages, lourd pour des couplages 0-D ; configuration XML.

## 10. Adaptateur SIMORPH nécessaire

in-process via FFI (liaison dynamique LGPL) ou python-bridge ; container pour les solveurs couplés.

## 11. Tests d'intégration prévus

Tutoriel « partitioned heat conduction » (deux FEniCSx couplés) vs solution monolithique.

## 12. Décision motivée

**Adapter (phase 2, multiphysique à maillage).** LGPL gérée par liaison dynamique.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web (precice.org, GitHub, Wikipedia).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.