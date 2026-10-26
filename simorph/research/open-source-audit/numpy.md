# NumPy

**Catégorie :** Modélisation scientifique  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/numpy/numpy

## 2. Fonctionnalités pertinentes pour SIMORPH

Tableaux N-D, algèbre linéaire de base, FFT, aléatoire ; socle de tout l'écosystème Python scientifique.

## 3. Licence et restrictions

BSD-3-Clause AND 0BSD AND MIT AND Zlib AND CC0-1.0 (expression SPDX du paquet `numpy` 2.5.4) — composants vendus sous licences permissives.

## 4. Activité de maintenance

Très actif (2.5.4, 2026-10-10).

## 5. Dépendances

Aucune (BLAS/LAPACK via OpenBLAS vendu).

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python natif ; PyO3/numpy crate pour in-process si besoin ; Arrow pour échanges volumineux.

## 7. Besoins CPU/GPU

CPU (multi-thread BLAS).

## 8. Interfaces disponibles

API Python, C-API.

## 9. Limites connues

Pas de GPU ; performance limitée aux opérations vectorisées.

## 10. Adaptateur SIMORPH nécessaire

python-bridge (déjà utilisé par adapters/python/examples/numpy_linear_solver.py).

## 11. Tests d'intégration prévus

Résolution d'un système linéaire de référence (matrice de Hilbert n=8) comparée à la solution exacte rationnelle ; déjà prévu dans le test d'intégration python-bridge.

## 12. Décision motivée

**Intégrer.** Indispensable côté Python.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.