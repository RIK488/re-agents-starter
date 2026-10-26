# LangGraph — LangChain

**Catégorie :** Agents et développement autonome  
**Décision :** adapter (optionnel)  

## 1. Dépôt officiel

https://github.com/langchain-ai/langgraph

## 2. Fonctionnalités pertinentes pour SIMORPH

Orchestration d'agents sous forme de graphes d'états avec persistance (checkpoints), reprise, humain-dans-la-boucle, streaming. Utile pour les workflows LLM du directeur scientifique côté Python.

## 3. Licence et restrictions

MIT (PyPI `langgraph` 1.2.18, 2026-10-26).

## 4. Activité de maintenance

Très actif (release le jour de l'audit). Version 1.x stable.

## 5. Dépendances

Python ≥3.10, langchain-core, pydantic ; LangGraph Platform/Studio sont des produits commerciaux séparés.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Bibliothèque Python ; aucun équivalent Rust. Le scheduler/orchestrateur SIMORPH est en Rust (simorph-scheduler) : LangGraph ne serait utilisé que pour des sous-workflows LLM côté Python.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python ; API JS séparée ; serveur HTTP via LangGraph Platform (commercial).

## 9. Limites connues

Risque de double orchestration (Rust + LangGraph) ; dépendance à l'écosystème LangChain ; pas de notion de ressources de calcul scientifique.

## 10. Adaptateur SIMORPH nécessaire

python-bridge (subprocess JSON-lines) : un graphe LangGraph exposé comme Component.

## 11. Tests d'intégration prévus

Graphe à 3 nœuds (formuler hypothèse → choisir simulateur → résumer) piloté par un LLM factice déterministe ; vérifier reprise après interruption via checkpoint.

## 12. Décision motivée

**Adapter (optionnel).** L'orchestration principale reste Rust ; LangGraph est accepté comme bibliothèque côté Python pour des sous-agents LLM, remplaçable par CrewAI/AutoGen.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.