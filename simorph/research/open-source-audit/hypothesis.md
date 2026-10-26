# Hypothesis

**Catégorie :** Vérification formelle / tests  
**Décision :** intégrer (tests)  

## 1. Dépôt officiel

https://github.com/HypothesisWorks/hypothesis

## 2. Fonctionnalités pertinentes pour SIMORPH

Tests basés sur les propriétés (property-based testing) en Python avec rétrécissement des contre-exemples, stratégies NumPy/pandas.

## 3. Licence et restrictions

MPL-2.0 (PyPI `hypothesis` 6.170.12, 2026-10-26). Copyleft faible par fichier : utilisation en dépendance de test sans contrainte.

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

attrs, sortedcontainers.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Côté Python uniquement (tests des adaptateurs Python) ; équivalent Rust : `proptest` (MIT/Apache) pour les crates.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python (pytest).

## 9. Limites connues

Python only.

## 10. Adaptateur SIMORPH nécessaire

Aucun (outil de test).

## 11. Tests d'intégration prévus

Propriétés du protocole JSON-lines de adapters/python (round-trip de TaskRequest/TaskResult aléatoires).

## 12. Décision motivée

**Intégrer (outil de test Python) ; proptest côté Rust.**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.