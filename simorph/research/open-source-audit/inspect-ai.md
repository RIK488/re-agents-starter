# Inspect AI — UK AI Security Institute

**Catégorie :** Évaluation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/UKGovernmentBEIS/inspect_ai

## 2. Fonctionnalités pertinentes pour SIMORPH

Framework d'évaluation de LLM : tâches, solveurs, scorers, bacs à sable (Docker), journaux détaillés, évaluations agentiques.

## 3. Licence et restrictions

MIT (PyPI `inspect-ai` 0.3.285, 2026-10-25 ; CITATION.cff).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Python, API LLM, Docker optionnel.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; journaux JSON exploitables par simorph-provenance.

## 7. Besoins CPU/GPU

CPU ; coût LLM.

## 8. Interfaces disponibles

Python, CLI, viewer web.

## 9. Limites connues

Orienté LLM/agents ; pas d'évaluation de modèles de contrôle.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Tâche d'évaluation à réponses vérifiables (arithmétique) sur un modèle local via llama.cpp ; scores identiques entre deux runs.

## 12. Décision motivée

**Intégrer.** Cadre d'évaluation principal de evaluation-service pour les LLM/agents.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json) ; vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.