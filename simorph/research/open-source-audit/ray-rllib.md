# Ray RLlib

**Catégorie :** Entraînement des IA  
**Décision :** adapter (phase 2)  

## 1. Dépôt officiel

https://github.com/ray-project/ray (rllib)

## 2. Fonctionnalités pertinentes pour SIMORPH

Bibliothèque RL distribuée (PPO, SAC, DQN, IMPALA, multi-agent), environnements Gymnasium/PettingZoo.

## 3. Licence et restrictions

Apache-2.0 (PyPI `ray` 2.59.0).

## 4. Activité de maintenance

Actif (RLlib en refonte « new API stack »).

## 5. Dépendances

Ray, PyTorch, Gymnasium.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; environnements SIMORPH exposés via Gymnasium.

## 7. Besoins CPU/GPU

CPU/GPU.

## 8. Interfaces disponibles

Python, CLI.

## 9. Limites connues

API en transition, lourde pour de petits problèmes.

## 10. Adaptateur SIMORPH nécessaire

python-bridge + Ray cluster.

## 11. Tests d'intégration prévus

PPO sur un environnement de contrôle d'attitude (plugins/physics via pont) : erreur de pointage finale < seuil sur 3 seeds.

## 12. Décision motivée

**Adapter (phase 2).** RL pour le contrôle et l'optimisation de politiques.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.