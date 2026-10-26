# OpenTelemetry

**Catégorie :** Infrastructure  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/open-telemetry

## 2. Fonctionnalités pertinentes pour SIMORPH

Traces, métriques, logs standardisés ; collecteur ; SDK Rust (`opentelemetry`, `tracing-opentelemetry`).

## 3. Licence et restrictions

Apache-2.0 (PyPI `opentelemetry-api` 1.45.1 ; SDK Rust Apache-2.0).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Collecteur optionnel.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Natif Rust via tracing ; simorph-telemetry exporte déjà Prometheus ; ajout d'un exporter OTLP.

## 7. Besoins CPU/GPU

Négligeable.

## 8. Interfaces disponibles

OTLP gRPC/HTTP.

## 9. Limites connues

Cardinalité des métriques.

## 10. Adaptateur SIMORPH nécessaire

in-process.

## 11. Tests d'intégration prévus

Spans d'une expérience exportés vers un collecteur local et relus.

## 12. Décision motivée

**Intégrer.**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.