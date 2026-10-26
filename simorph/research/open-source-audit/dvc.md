# DVC

**Catégorie :** Données et expérimentation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/iterative/dvc

## 2. Fonctionnalités pertinentes pour SIMORPH

Versionnage de données et pipelines sur Git, stockage distant (S3, GCS…), reproduction `dvc repro`.

## 3. Licence et restrictions

Apache-2.0 (PyPI `dvc` 3.67.1).

## 4. Activité de maintenance

Actif.

## 5. Dépendances

Python, Git.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

CLI ; hash des données compatibles avec Attachment.sha256 de ProblemSpec.

## 7. Besoins CPU/GPU

CPU/réseau.

## 8. Interfaces disponibles

CLI, Python.

## 9. Limites connues

Pipelines YAML distincts du scheduler SIMORPH.

## 10. Adaptateur SIMORPH nécessaire

subprocess.

## 11. Tests d'intégration prévus

Jeu de données versionné, hash DVC = sha256 enregistré dans la provenance.

## 12. Décision motivée

**Intégrer (versionnage des datasets).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.