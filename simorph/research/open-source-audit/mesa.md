# Mesa

**Catégorie :** Simulation multiagents  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/projectmesa/mesa

## 2. Fonctionnalités pertinentes pour SIMORPH

Simulation à base d'agents en Python : espaces (grille, continu, réseau), ordonnanceurs, collecte de données, visualisation Solara.

## 3. Licence et restrictions

Apache-2.0 (PyPI `mesa` 3.5.1, 2026-03-15).

## 4. Activité de maintenance

Actif (3.x).

## 5. Dépendances

NumPy, pandas, networkx, Solara.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; le noyau multiagent minimal de simorph-simulation couvre les cas simples en Rust.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python.

## 9. Limites connues

Performance Python pure (dizaines de milliers d'agents max).

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Modèle de Schelling seedé : ségrégation finale reproductible ; modèle SIR vs solution ODE SIR de simorph-simulation en limite de grande population.

## 12. Décision motivée

**Intégrer.** Moteur ABM de référence côté Python.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.