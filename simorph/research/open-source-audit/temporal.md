# Temporal

**Catégorie :** Infrastructure  
**Décision :** adapter (phase 3)  

## 1. Dépôt officiel

https://github.com/temporalio/temporal

## 2. Fonctionnalités pertinentes pour SIMORPH

Orchestration durable de workflows (reprise, retries, timers, historique), SDK multi-langages.

## 3. Licence et restrictions

MIT (serveur, SDK Python `temporalio` 1.34.0, CLI). Bundles tiers (ex. Elasticsearch) sous d'autres licences.

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Serveur Go + base (Postgres/Cassandra).

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

SDK Rust (core) existant ; alternative à l'exécuteur maison pour la durabilité des campagnes longues.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

gRPC, SDK Go/Java/Python/TS/.NET/Rust (core).

## 9. Limites connues

Service supplémentaire à opérer ; modèle de workflow déterministe à respecter.

## 10. Adaptateur SIMORPH nécessaire

gRPC (SDK Rust).

## 11. Tests d'intégration prévus

Campagne du directeur scientifique interrompue puis reprise : identique au snapshot/resume de simorph-scheduler.

## 12. Décision motivée

**Adapter (phase 3).** simorph-scheduler couvre la phase 1-2 ; Temporal pour la durabilité multi-jours.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json) ; vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.