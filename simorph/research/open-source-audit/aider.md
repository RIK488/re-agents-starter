# Aider

**Catégorie :** Agents et développement autonome  
**Décision :** adapter (optionnel)  

## 1. Dépôt officiel

https://github.com/Aider-AI/aider

## 2. Fonctionnalités pertinentes pour SIMORPH

Assistant de programmation en terminal avec carte du dépôt, édition multi-fichiers, intégration Git, mode scriptable.

## 3. Licence et restrictions

Apache-2.0 (classifieur PyPI `aider-chat` 0.86.2, 2026-02-12).

## 4. Activité de maintenance

Actif (release février 2026).

## 5. Dépendances

Python, Git, API LLM.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Scriptable (`aider --message`), léger ; bon candidat pour simulator-factory en bac à sable.

## 7. Besoins CPU/GPU

CPU ; coût LLM.

## 8. Interfaces disponibles

CLI, API Python (scripting).

## 9. Limites connues

Pas de bac à sable intégré ; qualité dépend du LLM.

## 10. Adaptateur SIMORPH nécessaire

Container + subprocess.

## 11. Tests d'intégration prévus

Génération d'un adaptateur Python pour un composant à partir du PROTOCOL.md, validé par le test d'intégration python-bridge.

## 12. Décision motivée

**Adapter (optionnel).** Outil de génération de code le plus simple à isoler ; retenu comme première option de simulator-factory avant OpenHands.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.