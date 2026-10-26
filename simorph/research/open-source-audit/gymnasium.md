# Gymnasium — Farama

**Catégorie :** Simulation multiagents / RL  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/Farama-Foundation/Gymnasium

## 2. Fonctionnalités pertinentes pour SIMORPH

API standard d'environnements RL (reset/step), environnements classiques, wrappers, vectorisation.

## 3. Licence et restrictions

MIT (PyPI `gymnasium` 1.4.0, 2026-10-05).

## 4. Activité de maintenance

Actif.

## 5. Dépendances

NumPy.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

API simple à refléter en Rust (trait Environment dans simorph-optimization RL) ; environnements Python via pont.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python.

## 9. Limites connues

Pas de multi-agent (voir PettingZoo).

## 10. Adaptateur SIMORPH nécessaire

python-bridge ; trait Rust miroir.

## 11. Tests d'intégration prévus

CartPole avec politique aléatoire seedée : retour moyen identique entre deux exécutions.

## 12. Décision motivée

**Intégrer.** Contrat standard pour les composants RL.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.