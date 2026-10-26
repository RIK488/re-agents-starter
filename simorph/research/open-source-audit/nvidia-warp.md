# NVIDIA Warp

**Catégorie :** Simulation physique et robotique  
**Décision :** intégrer (≥1.6)  

## 1. Dépôt officiel

https://github.com/NVIDIA/warp

## 2. Fonctionnalités pertinentes pour SIMORPH

Noyaux Python compilés CUDA/CPU, différentiables ; simulation (particules, tissus, rigides, FEM), géométrie, BVH.

## 3. Licence et restrictions

Apache-2.0 (PyPI `warp-lang` 1.18.0, 2026-10-05). Historique : licence propriétaire « NVIDIA Software License » jusqu'à début 2025, passage à Apache-2.0 (v1.6.x, mars 2025) après critiques de la communauté. Les versions antérieures restent sous l'ancienne licence : ne pas épingler < 1.6.

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Python, NumPy ; CUDA toolkit pour GPU (CPU fonctionne).

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; sorties différentiables pour optimisation.

## 7. Besoins CPU/GPU

GPU NVIDIA recommandé ; CPU possible.

## 8. Interfaces disponibles

API Python.

## 9. Limites connues

Écosystème NVIDIA ; portabilité GPU limitée à CUDA.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Chaîne masse-ressort 1D Warp vs RK4 Rust (écart < 1e-4 sur 10 s) ; gradient d/dk de la position finale vs différences finies.

## 12. Décision motivée

**Intégrer (optionnel GPU).** Version ≥ 1.6 uniquement.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json) ; changement de licence : vérifié le 2026-10-26 par recherche web (heise.de, docs officielles).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.