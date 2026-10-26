# Kratos Multiphysics — CIMNE

**Catégorie :** Complémentaire : multiphysique FEM  
**Décision :** adapter (optionnel)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/KratosMultiphysics/Kratos

## 2. Fonctionnalités pertinentes pour SIMORPH

Framework FEM multiphysique (structures, fluides, DEM, FSI, optimisation de forme) avec interface Python étendue ; CoSimulationApplication (couplage interne et FMI/preCICE).

## 3. Licence et restrictions

BSD-4-Clause pour le cœur et la plupart des applications (README) ; certaines applications dérivées peuvent avoir des licences propriétaires — vérifier par application.

## 4. Activité de maintenance

Actif.

## 5. Dépendances

C++, Python, Boost ; MPI optionnel.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

pip `KratosMultiphysics` ; pont Python.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Python, C++.

## 9. Limites connues

BSD-4 (clause publicitaire) incompatible avec GPL si mélange ; applications hétérogènes.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Poutre encastrée : flèche vs formule d'Euler-Bernoulli.

## 12. Décision motivée

**Adapter (optionnel, phase 3).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.