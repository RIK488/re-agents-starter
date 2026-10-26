# FMPy — Dassault Systèmes

**Catégorie :** Cosimulation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/CATIA-Systems/FMPy

## 2. Fonctionnalités pertinentes pour SIMORPH

Simulation d'FMU (FMI 1/2/3, ME/CS), validation d'FMU, GUI, cross-check, compilation de FMU sources.

## 3. Licence et restrictions

BSD-3-Clause (connaissance stable ; le champ licence du paquet PyPI `fmpy` 0.3.32 est vide — vérifier le dépôt).

## 4. Activité de maintenance

Actif (0.3.32, 2026-09-09).

## 5. Dépendances

NumPy, lxml, pywin32 (Windows) ; compilateur C pour FMU sources.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python : FmuComponent de adapters/fmi génère un script FMPy.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python, CLI, GUI.

## 9. Limites connues

Pas de cosimulation multi-FMU élaborée (voir OMSimulator/preCICE) ; Python only.

## 10. Adaptateur SIMORPH nécessaire

python-bridge (déjà prévu dans adapters/fmi).

## 11. Tests d'intégration prévus

FMU de référence FMI 3.0 (BouncingBall) : temps des rebonds vs simorph-simulation ; `fmpy validate` sur chaque FMU importé.

## 12. Décision motivée

**Intégrer.** Exécuteur FMU de phase 1.

---

**Dernière vérification :** 2026-10-26 — paquet PyPI vérifié (champ vide) ; licence : connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.