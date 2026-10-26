# pymoo

**Catégorie :** Complémentaire : optimisation multiobjectif  
**Décision :** intégrer (contrôle croisé)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/anyoptimization/pymoo

## 2. Fonctionnalités pertinentes pour SIMORPH

NSGA-II/III, MOEA/D, R-NSGA, hypervolume, problèmes de test ZDT/DTLZ, prise de décision multicritère.

## 3. Licence et restrictions

Apache-2.0 (PyPI `pymoo` 0.6.2).

## 4. Activité de maintenance

Actif.

## 5. Dépendances

NumPy, SciPy.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; oracle de validation de NSGA-II Rust.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Python.

## 9. Limites connues

Python only.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

ZDT1/ZDT3/DTLZ2 : hypervolume pymoo vs simorph-optimization à budget égal (écart < 5 %).

## 12. Décision motivée

**Intégrer (contrôle croisé).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.