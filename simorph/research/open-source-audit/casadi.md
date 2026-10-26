# CasADi

**Catégorie :** Complémentaire : optimisation différentiable / commande  
**Décision :** intégrer (phase 2)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/casadi/casadi

## 2. Fonctionnalités pertinentes pour SIMORPH

Différentiation algorithmique symbolique, interfaces IPOPT/SNOPT/qpOASES, intégrateurs (SUNDIALS), formulation de commande optimale (MPC, collocation).

## 3. Licence et restrictions

LGPL-3.0-or-later (PyPI `casadi` 3.8.1) ; les solveurs liés (IPOPT EPL, SNOPT propriétaire) ont leurs propres licences.

## 4. Activité de maintenance

Actif.

## 5. Dépendances

IPOPT, SUNDIALS, etc. embarqués dans les roues.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; C++ via FFI (LGPL dynamique).

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

Python, MATLAB, C++.

## 9. Limites connues

LGPL ; licences des solveurs tiers.

## 10. Adaptateur SIMORPH nécessaire

python-bridge.

## 11. Tests d'intégration prévus

Commande optimale du pendule inversé (collocation) : coût vs référence publiée ; MPC d'attitude vs PD de plugins/physics.

## 12. Décision motivée

**Intégrer (phase 2).** Optimisation sous contraintes différentiable et commande optimale (§3.7).

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.