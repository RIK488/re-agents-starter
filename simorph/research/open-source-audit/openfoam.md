# OpenFOAM (Foundation openfoam.org / OpenCFD openfoam.com)

**Catégorie :** Complémentaire : CFD  
**Décision :** adapter (conteneur, phase 3)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/OpenFOAM/OpenFOAM-dev ; https://develop.openfoam.com/Development/openfoam

## 2. Fonctionnalités pertinentes pour SIMORPH

CFD volumes finis (incompressible, compressible, multiphase, combustion, transfert thermique), parallèle MPI, adaptateur preCICE.

## 3. Licence et restrictions

GPL-3.0-or-later (copyleft fort). Deux lignées : Foundation (v14, 2026) et OpenCFD/ESI (v2606, 2026) ; marque déposée par OpenCFD.

## 4. Activité de maintenance

Très actif (deux releases/an).

## 5. Dépendances

C++, MPI, Scotch, ParaView pour post-traitement.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

**Jamais lié au noyau (GPL)** ; exécution en conteneur séparé pilotée par subprocess/fichiers ; couplage via preCICE.

## 7. Besoins CPU/GPU

CPU cluster.

## 8. Interfaces disponibles

CLI, dictionnaires de cas, PyFoam (tiers).

## 9. Limites connues

GPL ; courbe d'apprentissage ; pas d'API stable.

## 10. Adaptateur SIMORPH nécessaire

Container + subprocess (JSON-lines wrapper).

## 11. Tests d'intégration prévus

Cavité entraînée (lid-driven cavity, Re=100) vs données de Ghia et al. (1982).

## 12. Décision motivée

**Adapter (conteneur isolé, phase 3).** GPL imposé par l'isolation processus.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.