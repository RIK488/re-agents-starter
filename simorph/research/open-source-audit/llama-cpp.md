# llama.cpp

**Catégorie :** Infrastructure (inférence)  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/ggml-org/llama.cpp

## 2. Fonctionnalités pertinentes pour SIMORPH

Inférence LLM CPU/GPU en C/C++ (GGUF, quantification), serveur HTTP compatible OpenAI.

## 3. Licence et restrictions

MIT (PyPI `llama-cpp-python` 0.3.36 ; dépôt MIT).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

C/C++ ; CUDA/Metal/Vulkan optionnels.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

HTTP ou crate Rust `llama-cpp-2` (FFI) in-process.

## 7. Besoins CPU/GPU

CPU suffisant.

## 8. Interfaces disponibles

CLI, HTTP, C-API, Python.

## 9. Limites connues

Débit inférieur à vLLM sur GPU.

## 10. Adaptateur SIMORPH nécessaire

HTTP / in-process FFI.

## 11. Tests d'intégration prévus

Même test de déterminisme que vLLM sur un modèle 1B quantifié.

## 12. Décision motivée

**Intégrer (exécution locale §10.1).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.