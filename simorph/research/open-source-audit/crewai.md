# CrewAI

**Catégorie :** Agents et développement autonome  
**Décision :** écarter  

## 1. Dépôt officiel

https://github.com/crewAIInc/crewAI

## 2. Fonctionnalités pertinentes pour SIMORPH

Équipes d'agents à rôles, tâches, flows ; nombreux outils.

## 3. Licence et restrictions

MIT (fichier LICENSE du dépôt, connaissance stable) ; le champ licence du paquet PyPI `crewai` 1.15.30 est vide — vérifier le dépôt. CrewAI Enterprise/AMP est commercial.

## 4. Activité de maintenance

Très actif (release 2026-10-24).

## 5. Dépendances

Python ≥3.10, LiteLLM, pydantic, instructor…

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Python ; pont JSON-lines.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python, CLI.

## 9. Limites connues

Abstractions orientées « métier », peu de contrôle fin sur l'exécution ; dépendances lourdes.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Non prioritaire ; même test factice que LangGraph si retenu.

## 12. Décision motivée

**Écarter (redondant).** Une seule bibliothèque d'agents Python suffit en option ; LangGraph est préféré pour sa persistance.

---

**Dernière vérification :** 2026-10-26 — paquet PyPI vérifié le 2026-10-26 (champ licence vide) ; licence MIT : connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.