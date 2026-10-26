# SGLang

**Catégorie :** Infrastructure (inférence)  
**Décision :** écarter / alternative  

## 1. Dépôt officiel

https://github.com/sgl-project/sglang

## 2. Fonctionnalités pertinentes pour SIMORPH

Serveur d'inférence rapide (RadixAttention), langage frontal de programmation structurée, décodage contraint.

## 3. Licence et restrictions

Apache-2.0 (PyPI `sglang` 0.5.22).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

PyTorch, CUDA.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

HTTP.

## 7. Besoins CPU/GPU

GPU.

## 8. Interfaces disponibles

HTTP, Python.

## 9. Limites connues

Redondant avec vLLM.

## 10. Adaptateur SIMORPH nécessaire

HTTP.

## 11. Tests d'intégration prévus

Idem vLLM.

## 12. Décision motivée

**Écarter (redondant) ; alternative interchangeable via HTTP.**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.