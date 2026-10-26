# Three.js

**Catégorie :** Visualisation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/mrdoob/three.js

## 2. Fonctionnalités pertinentes pour SIMORPH

Rendu 3D WebGL/WebGPU ; scènes, caméras, chargeurs (glTF), post-traitement.

## 3. Licence et restrictions

MIT.

## 4. Activité de maintenance

Très actif (releases mensuelles).

## 5. Dépendances

Navigateur.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

apps/web (front) ; données servies par axum.

## 7. Besoins CPU/GPU

GPU client.

## 8. Interfaces disponibles

JS/ESM.

## 9. Limites connues

Pas de visualisation scientifique volumétrique avancée (voir ParaView/VTK.js).

## 10. Adaptateur SIMORPH nécessaire

Aucun (front-end).

## 11. Tests d'intégration prévus

Affichage de la trajectoire orbitale de plugins/physics (ECI) et de l'attitude (quaternion) ; test Playwright de rendu.

## 12. Décision motivée

**Intégrer.** Vue 3D de l'espace Simulation.

---

**Dernière vérification :** 2026-10-26 — connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.