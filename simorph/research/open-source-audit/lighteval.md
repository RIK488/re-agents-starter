# LightEval — Hugging Face

**Catégorie :** Évaluation  
**Décision :** écarter / contrôle croisé  

## 1. Dépôt officiel

https://github.com/huggingface/lighteval

## 2. Fonctionnalités pertinentes pour SIMORPH

Évaluation rapide multi-backends (transformers, vLLM, TGI, endpoints), tâches personnalisées.

## 3. Licence et restrictions

MIT (PyPI `lighteval` 0.13.0, 2025-11-24).

## 4. Activité de maintenance

Actif (dernière release novembre 2025 — à surveiller).

## 5. Dépendances

Transformers, datasets.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

CLI.

## 7. Besoins CPU/GPU

GPU.

## 8. Interfaces disponibles

CLI, Python.

## 9. Limites connues

Redondant avec lm-eval.

## 10. Adaptateur SIMORPH nécessaire

subprocess.

## 11. Tests d'intégration prévus

Même sous-ensemble GSM8K que lm-eval ; comparer les deux harnais (écart documenté).

## 12. Décision motivée

**Écarter (redondant) ; garder comme contrôle croisé.**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.