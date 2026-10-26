# Project Chrono

**Catégorie :** Simulation physique et robotique  
**Décision :** adapter (phase 2)  

## 1. Dépôt officiel

https://github.com/projectchrono/chrono

## 2. Fonctionnalités pertinentes pour SIMORPH

Multicorps, éléments finis (poutres/coques), véhicules, DEM granulaire, FSI (SPH), capteurs, cosimulation ; PyChrono.

## 3. Licence et restrictions

BSD-3-Clause (site officiel, conda pychrono) ; version 10.0.0 (mars 2026). Attention : certains forks déclarent GPL-3 pour leur code ajouté.

## 4. Activité de maintenance

Actif (10.0.0 en 2026).

## 5. Dépendances

C++17, Eigen, Irrlicht/VSG optionnels, CUDA pour modules GPU.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

C++ : liaison Rust via cxx/bindgen possible mais coûteuse ; PyChrono via pont Python ; Chrono supporte FMI (export FMU via module chrono_fmi) — chemin FMI privilégié.

## 7. Besoins CPU/GPU

CPU ; GPU pour granulaire/FSI.

## 8. Interfaces disponibles

C++, Python, FMI (module), ROS.

## 9. Limites connues

Build complexe ; documentation inégale.

## 10. Adaptateur SIMORPH nécessaire

FMI (export FMU) ou python-bridge (PyChrono) ; container.

## 11. Tests d'intégration prévus

Pendule et véhicule sur plan incliné (accélération = g sin θ (1 − μ…) vs analytique) ; FMU Chrono chargé par adapters/fmi.

## 12. Décision motivée

**Adapter (phase 2).** Complément de MuJoCo pour véhicules, granulaire et FEM ; intégration FMI.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.