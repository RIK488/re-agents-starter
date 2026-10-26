# DifferentialEquations.jl — SciML (Julia)

**Catégorie :** Modélisation scientifique  
**Décision :** intégrer (phase 2)  

## 1. Dépôt officiel

https://github.com/SciML/DifferentialEquations.jl

## 2. Fonctionnalités pertinentes pour SIMORPH

Suite la plus complète de solveurs ODE/SDE/DAE/DDE/PDE-method-of-lines, événements, sensibilités adjointes, ensembles parallèles, GPU.

## 3. Licence et restrictions

MIT (paquets SciML DiffEqBase, OrdinaryDiffEq, SciMLBase vérifiés MIT ; DifferentialEquations.jl lui-même MIT, connaissance stable).

## 4. Activité de maintenance

Très actif (releases 2026).

## 5. Dépendances

Julia ≥1.10 ; nombreux paquets SciML.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pas d'adaptateur Julia à ce stade (adapters/julia prévu par le cahier) : passerait par un subprocess Julia parlant le protocole JSON-lines, ou par PythonCall/juliacall via le pont Python. Coût de démarrage Julia élevé (précompilation).

## 7. Besoins CPU/GPU

CPU ; GPU via CUDA.jl.

## 8. Interfaces disponibles

API Julia ; diffeqpy (Python) ; diffeqr (R).

## 9. Limites connues

Latence de démarrage ; écosystème Julia à déployer ; interopérabilité Rust limitée.

## 10. Adaptateur SIMORPH nécessaire

Nouveau : adapters/julia (subprocess JSON-lines) — à créer ; ou python-bridge via diffeqpy.

## 11. Tests d'intégration prévus

Benchmark raide (Robertson, HIRES) comparé aux solveurs implicites Rust ; mesure du temps et de l'erreur.

## 12. Décision motivée

**Intégrer (phase 2, via adaptateur Julia).** Référence de qualité pour la validation croisée des solveurs Rust ; non bloquant pour la phase 1.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web pour l'écosystème ; dépôt principal : connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.