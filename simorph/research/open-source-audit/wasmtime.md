# Wasmtime (Bytecode Alliance)

**Catégorie :** Complémentaire : exécution isolée  
**Décision :** intégrer (phase 2)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/bytecodealliance/wasmtime

## 2. Fonctionnalités pertinentes pour SIMORPH

Runtime WebAssembly/WASI en Rust, modèle de composants, limites mémoire/temps (fuel), bac à sable par capacités.

## 3. Licence et restrictions

Apache-2.0 WITH LLVM-exception (crate `wasmtime` 50.0.0, 2026-10-20).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Rust.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

In-process Rust natif : `ExecutionInterface::Wasm` du manifeste ; isolation forte des composants générés par simulator-factory sans conteneur.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Rust API, CLI, C-API.

## 9. Limites connues

Pas de GPU ; bibliothèques scientifiques à recompiler en wasm.

## 10. Adaptateur SIMORPH nécessaire

in-process.

## 11. Tests d'intégration prévus

Composant RK4 compilé en wasm32-wasi exécuté avec limite de fuel ; dépassement → erreur Timeout ; résultats identiques au natif.

## 12. Décision motivée

**Intégrer (phase 2).** Isolation légère du code généré (§10.4).

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via l'API crates.io.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.