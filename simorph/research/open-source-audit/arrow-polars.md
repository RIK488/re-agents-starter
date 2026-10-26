# Apache Arrow / Polars

**Catégorie :** Complémentaire : données  
**Décision :** intégrer (phase 2)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/apache/arrow-rs ; https://github.com/pola-rs/polars

## 2. Fonctionnalités pertinentes pour SIMORPH

Format colonnaire en mémoire zéro-copie (Arrow, IPC/Flight) ; DataFrames rapides en Rust (Polars) avec API Python.

## 3. Licence et restrictions

Arrow : Apache-2.0 (crate `arrow` 57.3.2) ; Polars : MIT (crate `polars` 0.55.2, PyPI 2.1.0).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Rust.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

In-process Rust ; échange de trajectoires volumineuses avec Python (pyarrow) sans JSON.

## 7. Besoins CPU/GPU

Mémoire.

## 8. Interfaces disponibles

Rust, Python, C data interface, Flight gRPC.

## 9. Limites connues

Aucune majeure.

## 10. Adaptateur SIMORPH nécessaire

in-process.

## 11. Tests d'intégration prévus

Trajectoire 1e6 points transmise Rust→Python par Arrow IPC et relue identique.

## 12. Décision motivée

**Intégrer (phase 2).** Remplace JSON pour les gros volumes du protocole.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via l'API crates.io ; vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.