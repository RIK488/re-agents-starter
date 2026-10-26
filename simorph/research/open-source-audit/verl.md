# VERL — ByteDance/volcengine

**Catégorie :** Entraînement des IA  
**Décision :** adapter (phase 3)  

## 1. Dépôt officiel

https://github.com/volcengine/verl

## 2. Fonctionnalités pertinentes pour SIMORPH

RLHF/RLVR à grande échelle (HybridFlow), PPO/GRPO distribués avec vLLM/SGLang pour le rollout.

## 3. Licence et restrictions

Apache-2.0 (PyPI `verl` 0.9.1).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Ray, vLLM/SGLang, PyTorch, FSDP/Megatron.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Container + Ray ; pont Python pour pilotage.

## 7. Besoins CPU/GPU

Multi-GPU.

## 8. Interfaces disponibles

CLI/Hydra, Python.

## 9. Limites connues

Complexité, multi-GPU requis.

## 10. Adaptateur SIMORPH nécessaire

Container + python-bridge.

## 11. Tests d'intégration prévus

Run GRPO minimal sur 2 GPU avec GSM8K-mini ; convergence de la récompense.

## 12. Décision motivée

**Adapter (phase 3, distribué).** TRL suffit en phase 1-2.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.