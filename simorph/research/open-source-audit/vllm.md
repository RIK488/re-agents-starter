# vLLM

**Catégorie :** Infrastructure (inférence)  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/vllm-project/vllm

## 2. Fonctionnalités pertinentes pour SIMORPH

Serveur d'inférence LLM haut débit (PagedAttention, batching continu), API compatible OpenAI, quantification.

## 3. Licence et restrictions

Apache-2.0 (PyPI `vllm` 0.32.0).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

PyTorch, CUDA/ROCm.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

HTTP (API OpenAI-compatible) depuis Rust.

## 7. Besoins CPU/GPU

GPU.

## 8. Interfaces disponibles

HTTP, Python.

## 9. Limites connues

GPU obligatoire.

## 10. Adaptateur SIMORPH nécessaire

HTTP.

## 11. Tests d'intégration prévus

Génération déterministe (temperature 0, seed) répétée : sorties identiques.

## 12. Décision motivée

**Intégrer (serveur d'inférence GPU).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.