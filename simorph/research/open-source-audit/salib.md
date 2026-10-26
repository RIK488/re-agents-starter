# SALib

**Catégorie :** Complémentaire : analyse de sensibilité  
**Décision :** intégrer (contrôle croisé)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/SALib/SALib

## 2. Fonctionnalités pertinentes pour SIMORPH

Sobol (Saltelli), Morris, FAST, Delta, PAWN, HDMR ; échantillonneurs associés.

## 3. Licence et restrictions

MIT (PyPI `SALib` 1.6.0).

## 4. Activité de maintenance

Actif.

## 5. Dépendances

NumPy, SciPy, pandas.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; oracle de simorph-uncertainty.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Python, CLI.

## 9. Limites connues

Python only.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Ishigami : indices SALib vs Rust (écart < 0,02 à N=8192).

## 12. Décision motivée

**Intégrer (contrôle croisé).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.