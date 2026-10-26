# Transformers — Hugging Face

**Catégorie :** Entraînement des IA  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/huggingface/transformers

## 2. Fonctionnalités pertinentes pour SIMORPH

Modèles pré-entraînés (texte, vision, audio, multimodal), Trainer, génération, intégration Hub.

## 3. Licence et restrictions

Apache-2.0 (PyPI `transformers` 5.21.0, 2026-10-26). **Les poids des modèles ont chacun leur propre licence** (Llama, Gemma, Qwen… : licences custom ou gated) — l'audit de licence doit porter sur chaque modèle utilisé, pas seulement sur la bibliothèque.

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

PyTorch, tokenizers, safetensors, huggingface_hub.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python.

## 7. Besoins CPU/GPU

GPU.

## 8. Interfaces disponibles

Python ; `candle`/`tokenizers` Rust par HF.

## 9. Limites connues

Licences de poids hétérogènes ; API évolutive (5.x).

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Chargement d'un petit modèle Apache-2.0 (ex. distilgpt2 ou SmolLM) et évaluation de perplexité sur un texte fixe, reproductible.

## 12. Décision motivée

**Intégrer.** Avec registre de licences de poids dans la provenance.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.