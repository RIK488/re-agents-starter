# SWE-agent — Princeton

**Catégorie :** Agents et développement autonome  
**Décision :** écarter / option  

## 1. Dépôt officiel

https://github.com/SWE-agent/SWE-agent

## 2. Fonctionnalités pertinentes pour SIMORPH

Interface agent-ordinateur (ACI) pour résoudre des issues GitHub ; configuration YAML ; mini-SWE-agent minimal.

## 3. Licence et restrictions

MIT (classifieur PyPI du paquet `sweagent` 0.0.1 — placeholder ancien ; le dépôt est MIT, connaissance stable).

## 4. Activité de maintenance

Actif (recherche, 2024-2026).

## 5. Dépendances

Python, Docker, API LLM.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

CLI ; conteneur.

## 7. Besoins CPU/GPU

CPU ; coût LLM.

## 8. Interfaces disponibles

CLI, API Python.

## 9. Limites connues

Orienté résolution d'issues ; moins adapté à la création de nouveaux simulateurs.

## 10. Adaptateur SIMORPH nécessaire

Container + python-bridge.

## 11. Tests d'intégration prévus

Correction d'un bug injecté dans un plugin SIMORPH (test rouge → vert) en bac à sable.

## 12. Décision motivée

**Écarter à court terme ; option.** Redondant avec OpenHands/Aider.

---

**Dernière vérification :** 2026-10-26 — PyPI vérifié (paquet placeholder) ; dépôt : connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.