# CodeCarbon

**Catégorie :** Complémentaire : coût énergétique  
**Décision :** intégrer (phase 2)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/mlco2/codecarbon

## 2. Fonctionnalités pertinentes pour SIMORPH

Mesure de la consommation énergétique et des émissions CO₂e des calculs (CPU/GPU via RAPL/NVML).

## 3. Licence et restrictions

MIT (PyPI `codecarbon` 3.3.2).

## 4. Activité de maintenance

Actif.

## 5. Dépendances

Python, pynvml.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Pont Python ; alimente la dimension « coût » du Tradeoff (§8) et le contrôle des coûts (§10.3).

## 7. Besoins CPU/GPU

Accès RAPL/NVML.

## 8. Interfaces disponibles

Python, CLI.

## 9. Limites connues

Précision dépendante du matériel ; facteurs d'émission régionaux.

## 10. Adaptateur SIMORPH nécessaire

python-bridge / in-process (lecture RAPL en Rust possible).

## 11. Tests d'intégration prévus

Mesure d'une expérience LHS de 1000 runs : énergie > 0, reproductible à ±20 %.

## 12. Décision motivée

**Intégrer (phase 2).** Coût énergétique mesuré dans les compromis.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 via les métadonnées PyPI (pypi.org/pypi/<pkg>/json).

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.