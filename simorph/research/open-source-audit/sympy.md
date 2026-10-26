# SymPy

**Catégorie :** Modélisation scientifique  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/sympy/sympy

## 2. Fonctionnalités pertinentes pour SIMORPH

Calcul symbolique : simplification, dérivation, résolution d'équations, génération de code (codegen C/Fortran/Rust partiel), unités physiques (sympy.physics.units), lambdify.

## 3. Licence et restrictions

BSD-3-Clause (classifieur « BSD License » sur PyPI `sympy` 1.14.0).

## 4. Activité de maintenance

Actif (1.14.0, 2025-04-27 ; cadence annuelle).

## 5. Dépendances

mpmath.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python : model-factory l'utilise pour dériver des jacobiens analytiques et générer le code des RHS ; vérification dimensionnelle croisée avec simorph-units.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

API Python.

## 9. Limites connues

Lent sur grosses expressions ; codegen Rust limité.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Dérivation symbolique du jacobien de Van der Pol et comparaison avec le jacobien numérique de simorph-simulation (écart < 1e-6) ; contrôle dimensionnel d'une équation E = ½ m v² contre simorph-units.

## 12. Décision motivée

**Intégrer.** Moteur symbolique de model-factory.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.