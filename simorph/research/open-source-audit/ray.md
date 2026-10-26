# Ray

**Catégorie :** Infrastructure  
**Décision :** adapter (phase 2)  

## 1. Dépôt officiel

https://github.com/ray-project/ray

## 2. Fonctionnalités pertinentes pour SIMORPH

Calcul distribué Python (tâches/acteurs), Ray Tune, Ray Train, Ray Serve, Data.

## 3. Licence et restrictions

Apache-2.0 (PyPI `ray` 2.59.0).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Python, gRPC, Redis optionnel.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pas d'API Rust : SIMORPH ordonnance en Rust (simorph-scheduler) et délègue à Ray les lots Python/GPU via un composant « RayPool » (python-bridge).

## 7. Besoins CPU/GPU

Cluster CPU/GPU.

## 8. Interfaces disponibles

Python, Java, C++ (limité), dashboard.

## 9. Limites connues

Double ordonnanceur à cadrer ; empreinte mémoire.

## 10. Adaptateur SIMORPH nécessaire

python-bridge + container.

## 11. Tests d'intégration prévus

Soumettre 100 évaluations d'un composant Python via Ray et comparer aux résultats séquentiels (identiques).

## 12. Décision motivée

**Adapter (phase 2, distribué Python/GPU).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.