# Bullet / PyBullet

**Catégorie :** Simulation physique et robotique  
**Décision :** écarter  

## 1. Dépôt officiel

https://github.com/bulletphysics/bullet3

## 2. Fonctionnalités pertinentes pour SIMORPH

Physique rigide et souple, collisions, PyBullet pour robotique/RL.

## 3. Licence et restrictions

zlib (PyPI `pybullet` 3.2.7, 2025-01-30).

## 4. Activité de maintenance

Maintenance faible (dernière release PyPI janvier 2025 ; développement principal ralenti depuis des années).

## 5. Dépendances

C++ ; Python.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

C++, Python, C-API partagée.

## 9. Limites connues

Précision de contacts inférieure à MuJoCo ; peu maintenu.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Non prioritaire.

## 12. Décision motivée

**Écarter.** Remplacé par MuJoCo (plus précis, maintenu) ; licence très permissive sans objection.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.