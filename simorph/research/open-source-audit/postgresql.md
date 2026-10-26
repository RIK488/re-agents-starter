# PostgreSQL

**Catégorie :** Données et expérimentation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/postgres/postgres (miroir) ; https://www.postgresql.org

## 2. Fonctionnalités pertinentes pour SIMORPH

SGBD relationnel, JSONB, extensions (pgvector, TimescaleDB), transactions ; stockage de la provenance, des expériences et des utilisateurs.

## 3. Licence et restrictions

PostgreSQL License (permissive, type MIT/BSD).

## 4. Activité de maintenance

Très actif (releases annuelles).

## 5. Dépendances

Aucune.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Rust : sqlx/tokio-postgres/diesel ; Python : psycopg.

## 7. Besoins CPU/GPU

CPU, disque.

## 8. Interfaces disponibles

SQL, wire protocol, nombreux pilotes.

## 9. Limites connues

Opérations analytiques lourdes moins efficaces que DuckDB.

## 10. Adaptateur SIMORPH nécessaire

in-process (pilote Rust) — nouveau backend de ProvenanceStore.

## 11. Tests d'intégration prévus

Round-trip du registre de provenance (JSON-lines → Postgres) avec vérification de chaîne.

## 12. Décision motivée

**Intégrer.** Stockage transactionnel de référence.

---

**Dernière vérification :** 2026-10-26 — connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.