# nalgebra / ndarray (Rust)

**Catégorie :** Complémentaire : algèbre linéaire Rust  
**Décision :** intégrer  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/dimforge/nalgebra ; https://github.com/rust-ndarray/ndarray

## 2. Fonctionnalités pertinentes pour SIMORPH

nalgebra : algèbre linéaire typée (décompositions, quaternions, géométrie) ; ndarray : tableaux N-D avec BLAS optionnel.

## 3. Licence et restrictions

nalgebra : Apache-2.0 (0.35.0) ; ndarray : MIT OR Apache-2.0 (0.17.2).

## 4. Activité de maintenance

Actifs.

## 5. Dépendances

Rust.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

In-process (déjà dépendance workspace : nalgebra 0.33).

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Rust.

## 9. Limites connues

Pas de GPU ; perfs BLAS à activer.

## 10. Adaptateur SIMORPH nécessaire

in-process.

## 11. Tests d'intégration prévus

Déjà couvert par les tests des crates uncertainty/optimization (moindres carrés, GP).

## 12. Décision motivée

**Intégrer (déjà fait).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via l'API crates.io.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.