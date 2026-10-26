# Crates Rust d'intégration d'EDO : diffsol, ode_solvers

**Catégorie :** Complémentaire : solveurs Rust  
**Décision :** intégrer (phase 2)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/martinjrobins/diffsol ; https://github.com/srenevey/ode-solvers

## 2. Fonctionnalités pertinentes pour SIMORPH

diffsol : ODE/DAE semi-explicites, BDF/SDIRK, jacobiens par AD (Enzyme) ou DSL DiffSL, JIT LLVM/Cranelift, sensibilités (JOSS 2026) ; ode_solvers : Dormand-Prince 5/8 et RK4 simples.

## 3. Licence et restrictions

diffsol : MIT (0.18.0, 2026-10-20) ; ode_solvers : Apache-2.0 (0.6.2).

## 4. Activité de maintenance

diffsol très actif ; ode_solvers modéré.

## 5. Dépendances

nalgebra/faer ; LLVM optionnel pour diffsol.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

In-process Rust : candidat pour remplacer les intégrateurs maison de simorph-simulation sur les DAE et pour les sensibilités.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Rust.

## 9. Limites connues

diffsol jeune (API mouvante) ; build LLVM optionnel lourd.

## 10. Adaptateur SIMORPH nécessaire

in-process.

## 11. Tests d'intégration prévus

Robertson avec diffsol BDF vs implicite maison vs SciPy Radau ; mesure temps/erreur ; DAE pendule index-réduit.

## 12. Décision motivée

**Intégrer diffsol (phase 2) comme backend DAE/raide ; écarter ode_solvers (couvert).**

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via l'API crates.io.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.