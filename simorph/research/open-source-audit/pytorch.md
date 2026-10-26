# PyTorch

**Catégorie :** Entraînement des IA  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/pytorch/pytorch

## 2. Fonctionnalités pertinentes pour SIMORPH

Tenseurs, autograd, modules, distribué (FSDP, DDP), torch.compile, TorchScript/ExecuTorch pour l'embarqué.

## 3. Licence et restrictions

BSD-3-Clause (dépôt) ; l'expression SPDX du paquet PyPI `torch` 2.14.1 agrège les licences des composants vendus (Apache-2.0, LLVM-exception, BSD-2…). Les roues CUDA embarquent des bibliothèques NVIDIA sous EULA NVIDIA (redistribution soumise à conditions).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

CUDA/ROCm optionnels, NumPy.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; `tch-rs`/`candle`/`burn` côté Rust pour inférence in-process.

## 7. Besoins CPU/GPU

GPU recommandé.

## 8. Interfaces disponibles

Python, C++ (libtorch), ONNX export.

## 9. Limites connues

Taille des roues ; EULA NVIDIA des composants CUDA.

## 10. Adaptateur SIMORPH nécessaire

python-bridge (training-service).

## 11. Tests d'intégration prévus

Régression linéaire seedée : paramètres retrouvés à 1e-4 ; déterminisme avec `torch.use_deterministic_algorithms(True)`.

## 12. Décision motivée

**Intégrer.** Socle du training-service.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json) ; BSD-3 du dépôt : connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.