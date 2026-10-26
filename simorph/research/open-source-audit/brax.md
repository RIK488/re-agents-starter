# Brax — Google

**Catégorie :** Simulation physique et robotique  
**Décision :** écarter (MJX)  

## 1. Dépôt officiel

https://github.com/google/brax

## 2. Fonctionnalités pertinentes pour SIMORPH

Physique rigide différentiable en JAX, massivement parallèle, pipelines RL intégrés (PPO, SAC).

## 3. Licence et restrictions

Apache-2.0 (PyPI `brax` 0.14.2, 2026-03-15).

## 4. Activité de maintenance

Actif, mais convergence vers MJX (MuJoCo XLA) affichée par l'équipe.

## 5. Dépendances

JAX, MuJoCo (pipeline MJX).

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python.

## 7. Besoins CPU/GPU

GPU/TPU conseillé.

## 8. Interfaces disponibles

API Python.

## 9. Limites connues

Précision physique inférieure à MuJoCo ; avenir dépendant de MJX.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Entraînement PPO court sur `inverted_pendulum` seedé, score reproductible ±5 %.

## 12. Décision motivée

**Écarter au profit de MuJoCo/MJX.** Redondant.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.