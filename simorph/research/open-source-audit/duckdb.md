# DuckDB

**Catégorie :** Données et expérimentation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/duckdb/duckdb

## 2. Fonctionnalités pertinentes pour SIMORPH

SGBD analytique embarqué (OLAP), Parquet/Arrow natif, SQL vectorisé ; idéal pour analyser les résultats d'expériences.

## 3. Licence et restrictions

MIT (classifieur PyPI `duckdb` 2.0.0, 2026-10-21).

## 4. Activité de maintenance

Très actif (2.0 en 2026).

## 5. Dépendances

Aucune.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Crate Rust `duckdb` (bindings libduckdb) in-process ; Arrow pour échanges.

## 7. Besoins CPU/GPU

CPU, mémoire.

## 8. Interfaces disponibles

SQL, Python, Rust, C.

## 9. Limites connues

Mono-processus par base en écriture.

## 10. Adaptateur SIMORPH nécessaire

in-process (crate duckdb).

## 11. Tests d'intégration prévus

Agrégation des RunRecords d'une expérience LHS (statistiques par facteur) comparée au calcul Rust de simorph-experiment.

## 12. Décision motivée

**Intégrer.** Analyse locale des résultats.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.