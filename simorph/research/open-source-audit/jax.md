# JAX — Google

**Catégorie :** Modélisation scientifique  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/jax-ml/jax

## 2. Fonctionnalités pertinentes pour SIMORPH

Différentiation automatique, JIT XLA, vectorisation (vmap), CPU/GPU/TPU ; base de Brax, Diffrax, Optax.

## 3. Licence et restrictions

Apache-2.0 (PyPI `jax` 0.12.0.1, 2026-10-21).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

jaxlib (XLA), NumPy ; CUDA pour GPU.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; optimisation différentiable (§3.7) via gradients exacts de simulateurs écrits en JAX.

## 7. Besoins CPU/GPU

CPU ou GPU/TPU ; compilation JIT coûteuse au premier appel.

## 8. Interfaces disponibles

API Python.

## 9. Limites connues

Paradigme fonctionnel pur, courbe d'apprentissage ; pas d'interface Rust directe.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Gradient d'un simulateur masse-ressort JAX vs différences finies de simorph-uncertainty ; optimisation différentiable d'un paramètre jusqu'à l'optimum connu.

## 12. Décision motivée

**Intégrer (optionnel GPU).** Fournisseur d'optimisation différentiable et de simulateurs différentiables.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.