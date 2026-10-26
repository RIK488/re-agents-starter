# R&D-Agent — Microsoft

**Catégorie :** Recherche scientifique autonome  
**Décision :** adapter  

## 1. Dépôt officiel

https://github.com/microsoft/RD-Agent

## 2. Fonctionnalités pertinentes pour SIMORPH

Boucle automatisée Recherche → Développement : proposition d'hypothèses, implémentation, exécution, retour d'expérience ; scénarios data science (Kaggle), finance quantitative, lecture d'articles → modèles. Boucle « R&D loop » proche de §5.1.

## 3. Licence et restrictions

MIT (classifieur « MIT License » sur PyPI, paquet `rdagent` 1.0.0 du 2026-09-23). Aucune restriction d'usage commercial.

## 4. Activité de maintenance

Très actif : release 1.0.0 en septembre 2026, releases fréquentes en 2025-2026.

## 5. Dépendances

Python ≥3.10, LiteLLM/API LLM, Docker pour l'exécution isolée des expériences, pandas, etc.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Modules Python importables ; pont Python JSON-lines viable. Les scénarios sont très orientés data science/Kaggle ; la boucle générique est réutilisable.

## 7. Besoins CPU/GPU

CPU ; GPU selon scénarios ; coût d'API LLM.

## 8. Interfaces disponibles

CLI (`rdagent`), API Python, UI Streamlit de suivi.

## 9. Limites connues

Fortement couplé aux API LLM et à Docker ; pas d'interface de simulation physique ; critères de succès définis par l'agent lui-même (contraire à §3.2 sans validateur externe).

## 10. Adaptateur SIMORPH nécessaire

python-bridge (subprocess JSON-lines) + container pour l'exécution des expériences.

## 11. Tests d'intégration prévus

Scénario « data science » sur un jeu de données synthétique à solution connue ; vérifier que les métriques rapportées par R&D-Agent sont recalculées par simorph-validation (evaluation-service) et que l'écart est < tolérance.

## 12. Décision motivée

**Adapter.** Licence MIT, maintenance active, boucle R&D proche du besoin. Intégré comme stratégie optionnelle du laboratoire IA (services/ai-laboratory) derrière le validateur indépendant ; n'est jamais juge de ses propres résultats.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.