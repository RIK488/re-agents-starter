# OpenModelica — OSMC

**Catégorie :** Modélisation scientifique  
**Décision :** adapter (conteneur, FMI)  

## 1. Dépôt officiel

https://github.com/OpenModelica/OpenModelica

## 2. Fonctionnalités pertinentes pour SIMORPH

Compilateur Modelica, export FMU (ME/CS, FMI 2 et 3), simulation d'équations hybrides DAE, bibliothèque standard Modelica (MSL), OMPython, OMSimulator (cosimulation FMI).

## 3. Licence et restrictions

Double licence : OSMC-PL (OSMC Public License, dont les versions récentes 1.8 remplacent GPLv3 par **AGPLv3** comme option publique) OU AGPL-3.0/GPL-3.0 selon la version ; certains fichiers du run-time ont une option BSD-3. L'OSMC-PL n'offre des conditions de type EPL (usage avec code propriétaire) qu'aux **membres** du consortium. Le paquet PyPI `OMPython` embarque l'OSMC-PL. Les **FMU générés** par OpenModelica embarquent du code run-time : vérifier la licence du run-time (option BSD pour certains fichiers) avant de distribuer un FMU.

## 4. Activité de maintenance

Actif (releases régulières, consortium).

## 5. Dépendances

Lourd : compilateur C, OMC, Qt pour OMEdit.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Via FMI (adapters/fmi) : les modèles Modelica compilés en FMU sont le chemin d'intégration privilégié ; OMPython via pont Python pour piloter la compilation.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

CLI omc, OMPython (ZMQ/CORBA), OMSimulator C-API/Python, FMI.

## 9. Limites connues

Copyleft fort (AGPL/GPL) pour le compilateur ; installation lourde ; messages d'erreur du compilateur parfois opaques.

## 10. Adaptateur SIMORPH nécessaire

FMI (FMU exportés) + container pour omc ; python-bridge pour OMPython.

## 11. Tests d'intégration prévus

Compiler un modèle MSL (ex. Modelica.Electrical…RLC ou un circuit thermique) en FMU CS 2.0, le charger via adapters/fmi, comparer avec le réseau thermique Rust de plugins/physics.

## 12. Décision motivée

**Adapter (isolé en conteneur, via FMI).** Jamais lié au noyau (copyleft) ; utilisé comme outil externe générant des FMU dont la licence est à vérifier avant redistribution.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web (openmodelica.org/osmc-pl, bylaws, user's guide) + PyPI ; divergence GPL/AGPL entre sources selon la version.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.