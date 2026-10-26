# PettingZoo — Farama

**Catégorie :** Simulation multiagents / RL  
**Décision :** intégrer (optionnel)  

## 1. Dépôt officiel

https://github.com/Farama-Foundation/PettingZoo

## 2. Fonctionnalités pertinentes pour SIMORPH

API multi-agent (AEC et parallèle), environnements classiques, wrappers.

## 3. Licence et restrictions

MIT (PyPI `pettingzoo` 1.27.0, 2026-08-13).

## 4. Activité de maintenance

Actif.

## 5. Dépendances

Gymnasium, NumPy.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python.

## 9. Limites connues

Peu d'environnements industriels.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Environnement `simple_spread` seedé, reproductibilité.

## 12. Décision motivée

**Intégrer (optionnel).** Avec Gymnasium pour les systèmes multiagents.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.