# HELM — Stanford CRFM

**Catégorie :** Évaluation  
**Décision :** adapter (optionnel)  

## 1. Dépôt officiel

https://github.com/stanford-crfm/helm

## 2. Fonctionnalités pertinentes pour SIMORPH

Évaluation holistique multi-métriques (exactitude, calibration, robustesse, équité, toxicité, efficacité), scénarios nombreux, tableaux de bord.

## 3. Licence et restrictions

Apache-2.0 (PyPI `crfm-helm` 0.5.16, 2026-04-30).

## 4. Activité de maintenance

Actif (releases 2026).

## 5. Dépendances

Lourd (nombreux extras).

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

CLI.

## 7. Besoins CPU/GPU

GPU/API.

## 8. Interfaces disponibles

CLI, Python, site de résultats.

## 9. Limites connues

Installation lourde ; orienté LLM.

## 10. Adaptateur SIMORPH nécessaire

subprocess/container.

## 11. Tests d'intégration prévus

Scénario MMLU sous-ensemble ; vérifier la cohérence avec lm-eval.

## 12. Décision motivée

**Adapter (optionnel).** Métriques de calibration/robustesse utiles à §5.1.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.