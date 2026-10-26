# FEniCSx (DOLFINx, UFL, Basix)

**Catégorie :** Complémentaire : éléments finis  
**Décision :** adapter (phase 2)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/FEniCS/dolfinx

## 2. Fonctionnalités pertinentes pour SIMORPH

Éléments finis haute performance avec formulation variationnelle symbolique (UFL), parallèle MPI, large bibliothèque d'éléments ; couplage preCICE.

## 3. Licence et restrictions

LGPL-3.0-or-later (DOLFINx ; UFL/Basix LGPL/MIT selon composant).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

PETSc, MPI, pybind11/nanobind, Basix, UFL, FFCx.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python (conteneur recommandé : build lourd) ; LGPL compatible par liaison dynamique.

## 7. Besoins CPU/GPU

CPU cluster.

## 8. Interfaces disponibles

Python, C++.

## 9. Limites connues

Installation lourde (conda/Docker) ; pas de maillage intégré (gmsh).

## 10. Adaptateur SIMORPH nécessaire

Container + python-bridge.

## 11. Tests d'intégration prévus

Poisson 2D solution manufacturée : erreur L2 d'ordre p+1 mesurée.

## 12. Décision motivée

**Adapter (phase 2, PDE).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.