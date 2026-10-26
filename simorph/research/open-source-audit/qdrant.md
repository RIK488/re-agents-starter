# Qdrant

**Catégorie :** Données et expérimentation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/qdrant/qdrant

## 2. Fonctionnalités pertinentes pour SIMORPH

Base vectorielle écrite en Rust (recherche ANN, filtres, payloads), gRPC/REST ; mémoire sémantique pour la connaissance (§11) et la recherche de composants/analogies (§9).

## 3. Licence et restrictions

Apache-2.0 (dépôt et client Python `qdrant-client` 1.20.0).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Rust ; binaire autonome ou Docker.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Écrit en Rust : client Rust natif (crate `qdrant-client`, gRPC).

## 7. Besoins CPU/GPU

CPU, mémoire.

## 8. Interfaces disponibles

REST, gRPC, clients Rust/Python.

## 9. Limites connues

Service à déployer ; embeddings à produire ailleurs.

## 10. Adaptateur SIMORPH nécessaire

HTTP/gRPC (client Rust).

## 11. Tests d'intégration prévus

Indexer les manifestes de composants et retrouver « solveur raide » par similarité.

## 12. Décision motivée

**Intégrer.** Index sémantique de simorph-knowledge.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json) ; vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.