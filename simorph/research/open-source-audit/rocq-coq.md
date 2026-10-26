# Rocq Prover (anciennement Coq)

**Catégorie :** Vérification formelle  
**Décision :** adapter (optionnel)  

## 1. Dépôt officiel

https://github.com/rocq-prover/rocq

## 2. Fonctionnalités pertinentes pour SIMORPH

Assistant de preuve (calcul des constructions inductives) ; bibliothèques mathématiques (MathComp, Coquelicot, Flocq pour l'arithmétique flottante — pertinent pour certifier des erreurs d'arrondi).

## 3. Licence et restrictions

LGPL-2.1-only (opam `coq-core` 9.x ; Wikipedia). Renommage Coq → Rocq effectif (versions 9.x, 2025-2026) avec binaires de compatibilité `coqc`.

## 4. Activité de maintenance

Actif (9.2.0 mars 2026).

## 5. Dépendances

OCaml, opam.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Subprocess ; même modèle de ProofArtifact que Lean.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

CLI, SerAPI/LSP.

## 9. Limites connues

Deux assistants de preuve doublent l'effort ; Lean 4 préféré pour Mathlib.

## 10. Adaptateur SIMORPH nécessaire

Subprocess dédié.

## 11. Tests d'intégration prévus

Rejouer une preuve Flocq sur l'erreur d'arrondi d'une somme flottante.

## 12. Décision motivée

**Adapter (optionnel).** Retenu uniquement pour les bibliothèques spécifiques (Flocq, CompCert-adjacent) ; Lean 4 est l'assistant principal.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.