# MuJoCo — Google DeepMind

**Catégorie :** Simulation physique et robotique  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/google-deepmind/mujoco

## 2. Fonctionnalités pertinentes pour SIMORPH

Dynamique multicorps avec contacts, actionneurs, capteurs ; MJX (JAX, GPU, différentiable) ; MuJoCo Warp ; visualiseur ; format MJCF/URDF.

## 3. Licence et restrictions

Apache-2.0 (PyPI `mujoco` 3.17.0, 2026-10-22).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

C ; bindings Python ; JAX pour MJX.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

C-API liable depuis Rust (crate `mujoco-rs` communautaire, à auditer) ou pont Python. Modèle XML = ModelDescriptor exportable.

## 7. Besoins CPU/GPU

CPU ; GPU pour MJX/Warp.

## 8. Interfaces disponibles

C-API, Python, (C#/Unity), MJX JAX.

## 9. Limites connues

Contacts souples (pas de contact rigide exact) ; pas de fluides.

## 10. Adaptateur SIMORPH nécessaire

python-bridge (phase 1) ; in-process via C-API (phase 2).

## 11. Tests d'intégration prévus

Pendule simple et double pendule : conservation d'énergie sans contact vs simorph-simulation ; chute libre avec contact : temps d'impact vs analytique (bouncing ball de simorph-simulation).

## 12. Décision motivée

**Intégrer.** Moteur physique principal pour la robotique.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.