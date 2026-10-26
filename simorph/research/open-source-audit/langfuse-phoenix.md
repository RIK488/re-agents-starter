# Langfuse / Arize Phoenix (observabilité LLM)

**Catégorie :** Complémentaire : observabilité IA  
**Décision :** adapter Langfuse / écarter Phoenix  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/langfuse/langfuse ; https://github.com/Arize-ai/phoenix

## 2. Fonctionnalités pertinentes pour SIMORPH

Traçage des appels LLM, évaluations, jeux de données, coûts ; Phoenix : évals et visualisation d'embeddings.

## 3. Licence et restrictions

Langfuse : MIT sauf dossiers `/ee` et `/web/src/ee` sous licence commerciale (fonctions entreprise gated par clé) ; Phoenix : **Elastic License 2.0 (ELv2), non OSI**, auto-hébergement autorisé mais interdiction d'offrir le logiciel en service managé ; `arize-phoenix-otel` relicencié Apache-2.0.

## 4. Activité de maintenance

Très actifs.

## 5. Dépendances

Serveur (Postgres/ClickHouse pour Langfuse).

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

OTLP/HTTP depuis simorph-telemetry.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

HTTP, SDK Python/JS, OpenTelemetry.

## 9. Limites connues

ELv2 de Phoenix ; Langfuse EE.

## 10. Adaptateur SIMORPH nécessaire

HTTP (OTLP).

## 11. Tests d'intégration prévus

Traces des appels LLM du directeur scientifique exportées vers Langfuse OSS et retrouvées par run_id.

## 12. Décision motivée

**Adapter Langfuse (édition MIT) ; écarter Phoenix (ELv2).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json) ; vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.