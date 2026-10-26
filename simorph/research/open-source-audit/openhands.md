# OpenHands (ex-OpenDevin)

**Catégorie :** Agents et développement autonome  
**Décision :** adapter (optionnel)  

## 1. Dépôt officiel

https://github.com/All-Hands-AI/OpenHands

## 2. Fonctionnalités pertinentes pour SIMORPH

Agent de développement logiciel autonome avec bac à sable (Docker), navigateur, terminal ; SDK agent.

## 3. Licence et restrictions

MIT (PyPI `openhands-ai` 1.11.0, 2026-07-09). Certaines parties de l'UI/Cloud sont commerciales (vérifier les sous-dossiers).

## 4. Activité de maintenance

Actif (1.x en 2026).

## 5. Dépendances

Python, Docker, API LLM.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Service conteneurisé avec API REST ; utilisable par simulator-factory pour générer/modifier du code de simulateur dans un bac à sable, les artefacts étant ensuite testés par simorph-validation.

## 7. Besoins CPU/GPU

CPU ; coût LLM.

## 8. Interfaces disponibles

API REST, CLI, SDK Python, UI web.

## 9. Limites connues

Lourd (Docker in Docker) ; sécurité : exécution de code généré — bac à sable obligatoire ; pas de garanties de correction.

## 10. Adaptateur SIMORPH nécessaire

Container + HTTP.

## 11. Tests d'intégration prévus

Tâche de génération d'un intégrateur RK4 en Rust à partir d'une spécification, puis compilation/tests par simulator-factory ; mesurer le taux de réussite sur 5 essais seedés.

## 12. Décision motivée

**Adapter (optionnel, phase ultérieure).** Candidat principal pour la génération assistée de code dans simulator-factory ; SWE-agent et Aider sont des alternatives plus légères.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.