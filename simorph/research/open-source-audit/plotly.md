# Plotly (plotly.js / plotly.py)

**Catégorie :** Visualisation  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/plotly/plotly.js ; https://github.com/plotly/plotly.py

## 2. Fonctionnalités pertinentes pour SIMORPH

Graphiques interactifs 2D/3D, incertitudes (bandes), surfaces, parallel coordinates (fronts de Pareto).

## 3. Licence et restrictions

MIT (PyPI `plotly` 7.1.0 ; plotly.js MIT).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Navigateur ; Python optionnel.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

apps/web (JSON de figures générées par Rust).

## 7. Besoins CPU/GPU

Client.

## 8. Interfaces disponibles

JS, Python.

## 9. Limites connues

Volumétrie limitée.

## 10. Adaptateur SIMORPH nécessaire

Aucun (front-end).

## 11. Tests d'intégration prévus

Front de Pareto ZDT1 de simorph-optimization tracé et comparé visuellement/numériquement au front de référence.

## 12. Décision motivée

**Intégrer.** Graphiques 2D/3D et incertitudes.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.