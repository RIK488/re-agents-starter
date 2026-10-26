# TRL — Hugging Face

**Catégorie :** Entraînement des IA  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/huggingface/trl

## 2. Fonctionnalités pertinentes pour SIMORPH

SFT, DPO, GRPO, PPO, reward modeling pour LLM.

## 3. Licence et restrictions

Apache-2.0 (PyPI `trl` 1.16.0).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Transformers, PEFT, accelerate.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python.

## 7. Besoins CPU/GPU

GPU.

## 8. Interfaces disponibles

Python, CLI.

## 9. Limites connues

Algorithmes RL pour LLM seulement.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

GRPO jouet sur une tâche arithmétique avec récompense vérifiable ; la récompense moyenne doit croître sur 3 seeds.

## 12. Décision motivée

**Intégrer.** Entraînement par renforcement des LLM (récompenses issues de simorph-validation).

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.