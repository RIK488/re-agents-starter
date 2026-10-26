# PEFT — Hugging Face

**Catégorie :** Entraînement des IA  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/huggingface/peft

## 2. Fonctionnalités pertinentes pour SIMORPH

LoRA, QLoRA, adapters, prompt tuning ; fine-tuning économe.

## 3. Licence et restrictions

Apache-2.0 (PyPI `peft` 0.21.2).

## 4. Activité de maintenance

Actif.

## 5. Dépendances

Transformers, PyTorch, bitsandbytes optionnel.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python.

## 7. Besoins CPU/GPU

GPU.

## 8. Interfaces disponibles

Python.

## 9. Limites connues

Dépend des versions Transformers.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

LoRA sur SmolLM avec 50 pas seedés : perte décroissante et checkpoints reproductibles (hash).

## 12. Décision motivée

**Intégrer.** Méthode d'entraînement par défaut du laboratoire IA (coût).

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.