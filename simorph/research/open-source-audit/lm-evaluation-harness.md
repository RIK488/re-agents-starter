# LM Evaluation Harness — EleutherAI

**Catégorie :** Évaluation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/EleutherAI/lm-evaluation-harness

## 2. Fonctionnalités pertinentes pour SIMORPH

Centaines de benchmarks académiques standardisés (MMLU, GSM8K, HellaSwag…), backends HF/vLLM/API.

## 3. Licence et restrictions

MIT (PyPI `lm-eval` 0.4.13).

## 4. Activité de maintenance

Actif.

## 5. Dépendances

Transformers, datasets.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

CLI/subprocess.

## 7. Besoins CPU/GPU

GPU pour modèles locaux.

## 8. Interfaces disponibles

CLI, Python.

## 9. Limites connues

Licences des datasets variées ; contamination des benchmarks.

## 10. Adaptateur SIMORPH nécessaire

python-bridge/subprocess.

## 11. Tests d'intégration prévus

GSM8K sous-ensemble 50 items sur un petit modèle : score reproductible.

## 12. Décision motivée

**Intégrer.** Benchmarks de référence.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.