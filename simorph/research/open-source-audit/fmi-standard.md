# FMI — Functional Mock-up Interface (Modelica Association)

**Catégorie :** Cosimulation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/modelica/fmi-standard ; https://fmi-standard.org

## 2. Fonctionnalités pertinentes pour SIMORPH

Standard d'échange de modèles (Model Exchange, Co-Simulation, Scheduled Execution en 3.0) ; FMU = zip avec modelDescription.xml + binaires ; FMI 3.0.1 (2023). Pilier de §3.5.

## 3. Licence et restrictions

Code (en-têtes C, schémas XML) : BSD-2-Clause ; documents de spécification : CC-BY-SA 4.0. Aucune restriction sur l'implémentation.

## 4. Activité de maintenance

Standard maintenu (3.0.1 ; travaux sur layered standards fmi-ls-*).

## 5. Dépendances

Aucune.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

adapters/fmi parse modelDescription.xml 2.0/3.0 en ComponentManifest ; exécution par FMPy (Python) ; un chargeur natif Rust (dlopen des binaires FMU) est une évolution possible.

## 7. Besoins CPU/GPU

Selon FMU.

## 8. Interfaces disponibles

C-API, XML.

## 9. Limites connues

Les FMU binaires sont spécifiques à la plateforme ; licence des FMU générés dépend de l'outil générateur (ex. OpenModelica).

## 10. Adaptateur SIMORPH nécessaire

FMI (natif SIMORPH).

## 11. Tests d'intégration prévus

Référence FMI Cross-Check : FMU « BouncingBall », « Dahlquist », « VanDerPol » (FMI 3.0 reference FMUs, BSD-2) exécutés via FMPy et comparés aux solutions Rust.

## 12. Décision motivée

**Intégrer.** Standard d'interopérabilité obligatoire.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web (fmi-standard.org, dépôt GitHub).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.