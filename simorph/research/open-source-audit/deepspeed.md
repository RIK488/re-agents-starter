# DeepSpeed — Microsoft / LF AI & Data

**Catégorie :** Entraînement des IA  
**Décision :** intégrer (optionnel)  

## 1. Dépôt officiel

https://github.com/deepspeedai/DeepSpeed (migration depuis microsoft/DeepSpeed)

## 2. Fonctionnalités pertinentes pour SIMORPH

ZeRO 1/2/3, offload, pipeline parallel, inference kernels, compression.

## 3. Licence et restrictions

Apache-2.0 (PyPI `deepspeed` 0.19.8) ; projet transféré à la LF AI & Data (org GitHub deepspeedai). Une ancienne mention MIT circule : obsolète.

## 4. Activité de maintenance

Actif.

## 5. Dépendances

PyTorch, CUDA, ninja.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python via accelerate.

## 7. Besoins CPU/GPU

Multi-GPU.

## 8. Interfaces disponibles

Python, launcher CLI.

## 9. Limites connues

Compilation d'ops CUDA à l'usage.

## 10. Adaptateur SIMORPH nécessaire

python-bridge/container.

## 11. Tests d'intégration prévus

ZeRO-2 sur SmolLM, 2 GPU : même perte que sans ZeRO à 1e-3.

## 12. Décision motivée

**Intégrer (optionnel multi-GPU).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json) ; transfert : vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.