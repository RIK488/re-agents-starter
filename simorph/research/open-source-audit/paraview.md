# ParaView / VTK — Kitware

**Catégorie :** Visualisation  
**Décision :** intégrer (phase 2)  

## 1. Dépôt officiel

https://gitlab.kitware.com/paraview/paraview ; https://github.com/Kitware/VTK

## 2. Fonctionnalités pertinentes pour SIMORPH

Visualisation scientifique volumétrique, maillages, champs, parallèle (Catalyst in situ), pvpython, VTK.js/trame pour le web ; ParaView 6.2 (2026).

## 3. Licence et restrictions

BSD-3-Clause (ParaView et VTK).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

C++, Qt, VTK ; Python.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

pvpython via pont Python ; export VTK/VTU depuis Rust (écriture XML simple) ; trame/VTK.js pour l'intégration web.

## 7. Besoins CPU/GPU

CPU/GPU.

## 8. Interfaces disponibles

GUI, Python, C++, Catalyst, web (trame).

## 9. Limites connues

Lourd pour des résultats 0-D/1-D.

## 10. Adaptateur SIMORPH nécessaire

python-bridge (pvpython) ; fichiers VTU.

## 11. Tests d'intégration prévus

Export d'un champ thermique 2D (plugin futur PDE) en VTU et rendu par pvpython en image de référence.

## 12. Décision motivée

**Intégrer (phase 2, champs/maillages).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.