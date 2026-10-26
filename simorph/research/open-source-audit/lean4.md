# Lean 4 (+ Mathlib) — Lean FRO

**Catégorie :** Vérification formelle  
**Décision :** intégrer (phase 2)  

## 1. Dépôt officiel

https://github.com/leanprover/lean4 ; https://github.com/leanprover-community/mathlib4

## 2. Fonctionnalités pertinentes pour SIMORPH

Assistant de preuve et langage ; Mathlib (> 1,5 M lignes) ; Lake ; export de certificats vérifiables par le noyau Lean.

## 3. Licence et restrictions

Apache-2.0 (Lean 4 et Mathlib).

## 4. Activité de maintenance

Très actif (4.35.0 octobre 2026 selon Wikipedia ; Lean FRO).

## 5. Dépendances

Toolchain elan/lake ; Mathlib (~ cache de plusieurs Go).

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Subprocess (`lake env lean`) ; SIMORPH déclare un `ProofArtifact { prover: Lean4 }` ; la vérification machine d'un certificat Lean consiste à rejouer `lean` sur le fichier en bac à sable — à implémenter dans simorph-validation (actuellement NotImplemented, documenté).

## 7. Besoins CPU/GPU

CPU, mémoire (Mathlib).

## 8. Interfaces disponibles

CLI, LSP, Lake.

## 9. Limites connues

Formaliser des résultats de physique numérique est coûteux ; le niveau « mathématiquement démontré » ne peut être accordé qu'à des énoncés effectivement formalisés.

## 10. Adaptateur SIMORPH nécessaire

Subprocess (JSON-lines) dédié : adapters/external-tools/lean.

## 11. Tests d'intégration prévus

Rejouer une preuve Mathlib simple (ex. irrationalité de √2) et un énoncé de SIMORPH (borne d'erreur locale d'Euler explicite) ; un certificat altéré doit échouer.

## 12. Décision motivée

**Intégrer (phase 2).** Seule voie vers `EvidenceLevel::MathematicallyProven` machine-vérifié.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.