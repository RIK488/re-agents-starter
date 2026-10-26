# Darwin Gödel Machine (DGM) — Sakana AI / UBC / Vector

**Catégorie :** Recherche scientifique autonome  
**Décision :** écarter (principes repris)  

## 1. Dépôt officiel

https://github.com/jennyzzt/dgm (dépôt officiel cité par l'article arXiv:2505.22954)

## 2. Fonctionnalités pertinentes pour SIMORPH

Agent de code auto-améliorant : archive ouverte de variantes d'agents, modifications de son propre code évaluées sur SWE-bench/Polyglot, sélection par performance mesurée. Illustre §5.2 (auto-amélioration contrôlée) avec son mécanisme d'archive et d'évaluation empirique.

## 3. Licence et restrictions

Apache-2.0 (section licence du dépôt affichée dans les résultats de recherche) ; l'article est CC BY 4.0. Nombreuses réimplémentations tierces sous MIT (lemoz/…, mmtmn/…) à ne pas confondre.

## 4. Activité de maintenance

Artefact de recherche (mai 2025) ; faible activité attendue après publication — à vérifier.

## 5. Dépendances

Python, Docker (exécution des agents), API LLM (Claude/o3 dans l'article), SWE-bench.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pas de bibliothèque : scripts. Intégration conteneurisée uniquement.

## 7. Besoins CPU/GPU

CPU ; coût LLM très élevé (plusieurs dizaines de milliers de USD rapportés pour une exécution complète).

## 8. Interfaces disponibles

CLI/scripts Python.

## 9. Limites connues

Coût ; sécurité : l'agent modifie son propre code — l'article rapporte des cas de « hallucination d'usage d'outils » et de contournement de métriques, d'où la nécessité d'un évaluateur protégé (§3.2, ProtectedReferences dans simorph-validation).

## 10. Adaptateur SIMORPH nécessaire

Container ; aucune intégration directe.

## 11. Tests d'intégration prévus

Aucun test d'intégration à court terme ; étude de conception seulement (archive d'agents, évaluation hors ligne).

## 12. Décision motivée

**Écarter comme dépendance, intégrer les principes.** Le mécanisme (archive + évaluation empirique + rollback) est repris dans services/ai-laboratory avec références de validation scellées ; le code lui-même est trop coûteux et trop couplé à SWE-bench.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.