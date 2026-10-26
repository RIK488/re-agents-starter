# AutoGen — Microsoft (et le fork communautaire AG2)

**Catégorie :** Agents et développement autonome  
**Décision :** écarter / option AG2  

## 1. Dépôt officiel

https://github.com/microsoft/autogen ; fork communautaire : https://github.com/ag2ai/ag2

## 2. Fonctionnalités pertinentes pour SIMORPH

Conversations multi-agents, exécution de code, agents outillés ; v0.4 : architecture événementielle/acteurs.

## 3. Licence et restrictions

Scission importante : AutoGen (Microsoft) `autogen-core` 0.7.5 : MIT (PyPI, 2025-09-30, dernière release) ; AG2 `ag2` 1.2.1 : Apache-2.0 (PyPI, 2026-10-21). Microsoft a placé AutoGen en mode maintenance (correctifs seulement) au profit de « Microsoft Agent Framework » (MIT, GA 2026). Le nom de paquet `pyautogen`/`autogen` sur PyPI appartient à AG2, pas à Microsoft.

## 4. Activité de maintenance

AutoGen : maintenance seulement (dernière release PyPI septembre 2025). AG2 : actif. Microsoft Agent Framework : actif.

## 5. Dépendances

Python ≥3.10 ; API LLM.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Python uniquement ; pont JSON-lines.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python ; AutoGen Studio (UI) ; .NET pour la version Microsoft.

## 9. Limites connues

Fragmentation (3 lignées), confusion de noms de paquets, maintenance incertaine de la lignée Microsoft.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Idem LangGraph (graphe d'agents factice) ; test de non-régression de l'import des deux paquets avec noms distincts.

## 12. Décision motivée

**Écarter AutoGen (maintenance), garder AG2 ou Microsoft Agent Framework en option.** Pas de rôle structurant : SIMORPH n'est pas un orchestrateur d'agents (§1).

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json) ; scission et mode maintenance : vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.