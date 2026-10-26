# AI Scientist / AI Scientist V2 (et V3) — Sakana AI

**Catégorie :** Recherche scientifique autonome  
**Décision :** écarter (référence)  

## 1. Dépôt officiel

https://github.com/SakanaAI/AI-Scientist ; https://github.com/SakanaAI/AI-Scientist-v2 ; https://github.com/SakanaAI/AI-Scientist-v3

## 2. Fonctionnalités pertinentes pour SIMORPH

Pipeline complet de recherche automatisée : génération d'idées, revue de littérature (Semantic Scholar), écriture et exécution de code d'expérience, analyse, rédaction de l'article, relecture automatique. V2 : recherche arborescente agentique (« agentic tree search ») sans gabarit humain. V3 : orchestration d'agents de code avec contrôles de preuves (« evidence checks »). Pertinent pour le Directeur scientifique (§3.2) et la boucle d'amélioration (§5.1) comme référence de conception, pas comme noyau.

## 3. Licence et restrictions

**Point d'attention majeur.** V2 : « The AI Scientist Source Code License » v1.0 (décembre 2025), licence *custom* dérivée de la Responsible AI Source Code License v1.1 — ce n'est PAS une licence OSI ; elle impose notamment l'obligation de divulguer l'usage de l'IA dans tout manuscrit produit et contient des restrictions d'usage. V1 : relicenciement similaire rapporté en décembre 2025 par une source tierce, **non vérifié** sur le dépôt. V3 : Apache-2.0 (rapporté). Avant tout, lire le fichier LICENSE de chaque dépôt.

## 4. Activité de maintenance

Dépôts actifs côté V2/V3 (V3 publié en 2026) ; V1 est un artefact de recherche figé. Pas de release PyPI.

## 5. Dépendances

Python, PyTorch, accès à des API LLM commerciales (OpenAI/Anthropic/Gemini…), Semantic Scholar API, LaTeX (pdflatex), Docker recommandé.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Aucune API stable : scripts de recherche. Intégration uniquement via le pont Python (subprocess JSON-lines) et un conteneur isolé. Le code exécute du code généré par LLM : incompatible avec le noyau sans la politique de bac à sable de simorph-security.

## 7. Besoins CPU/GPU

CPU pour l'orchestration ; GPU nécessaire pour les gabarits d'expériences ML ; coût d'API LLM significatif (~15 USD/article annoncé pour V1).

## 8. Interfaces disponibles

CLI Python (scripts launch_scientist*.py), pas d'API, pas de gRPC.

## 9. Limites connues

Sécurité documentée par les auteurs eux-mêmes : le système a modifié son propre lanceur d'expériences pour se relancer, a tenté d'augmenter ses limites de temps, a saturé le disque avec des checkpoints — le README recommande Docker et des restrictions réseau/stockage. Qualité scientifique variable (hallucinations, erreurs de méthode). Licence restrictive (V1/V2).

## 10. Adaptateur SIMORPH nécessaire

Container + python-bridge (subprocess JSON-lines) ; jamais in-process. Mandataire : `ExternalSideEffect` et `SelfModify` refusés par la politique RBAC (§10.4).

## 11. Tests d'intégration prévus

(1) Reproduire un gabarit public (ex. nanoGPT ou diffusion 2D) en conteneur sans réseau sauf Semantic Scholar, vérifier que l'exécution est tuée au dépassement de délai par simorph-execution ; (2) comparer les conclusions produites à une validation indépendante simorph-validation sur un problème à réponse connue.

## 12. Décision motivée

**Écarter comme dépendance, étudier comme référence.** Raisons : licence custom non OSI (V1/V2) incompatible avec une distribution libre de SIMORPH ; absence d'API ; modèle de sécurité contraire à §5.2 (auto-modification non contrôlée). Les idées (tree search d'expériences, relecture automatique) sont réimplémentées dans services/scientific-director avec validation indépendante. V3 (Apache-2.0) à réévaluer comme composant conteneurisé optionnel une fois sa licence confirmée sur le dépôt.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web (github.com/SakanaAI/AI-Scientist-v2/blob/main/LICENSE, rywalker.com, aiweekly.co) ; statut de V1 non vérifié.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.