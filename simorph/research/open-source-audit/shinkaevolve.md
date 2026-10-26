# ShinkaEvolve — Sakana AI

**Catégorie :** Recherche scientifique autonome  
**Décision :** adapter (sous réserve licence)  

## 1. Dépôt officiel

https://github.com/SakanaAI/ShinkaEvolve

## 2. Fonctionnalités pertinentes pour SIMORPH

Évolution de programmes guidée par LLM (style AlphaEvolve) avec forte efficacité en échantillons : échantillonnage de parents adaptatif, rejet des doublons par similarité, ensemble de LLM. Applicable à l'amélioration d'algorithmes, de solveurs et de stratégies (§5.2 auto-amélioration des méthodes de SIMORPH).

## 3. Licence et restrictions

Apache-2.0 rapporté par plusieurs sources secondaires ; **non vérifié sur le fichier LICENSE**. Attention : le paquet PyPI `shinka` (0.3.0, MIT, auteur « jordan legg ») est un projet SANS rapport — ne pas l'installer par erreur.

## 4. Activité de maintenance

Publié septembre 2025 ; activité 2025-2026 à confirmer.

## 5. Dépendances

Python, API LLM (OpenAI/Anthropic/Gemini/…), Hydra, WebUI optionnelle.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Installation depuis le dépôt Git ; pont Python JSON-lines ; l'évaluateur de programme est une fonction Python que SIMORPH peut fournir (bench de simulateur, solveur).

## 7. Besoins CPU/GPU

CPU ; coût LLM ; GPU seulement si la fonction évaluée en a besoin.

## 8. Interfaces disponibles

CLI Python, API Python, WebUI.

## 9. Limites connues

Jeune ; dépend d'API LLM externes ; pas de notion d'unités ni de validation scientifique — tout résultat est une hypothèse jusqu'à validation.

## 10. Adaptateur SIMORPH nécessaire

python-bridge (subprocess JSON-lines) en conteneur ; sorties enregistrées en provenance.

## 11. Tests d'intégration prévus

Faire évoluer une heuristique de placement/empilement sur un benchmark à optimum connu (ex. circle packing n=26) et vérifier le score par simorph-validation.

## 12. Décision motivée

**Adapter (après confirmation de licence).** Mécanisme d'évolution de programmes utile pour simulator-factory et l'auto-amélioration ; conditionné à la vérification Apache-2.0 sur le dépôt et à l'exécution en bac à sable.

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web (sources secondaires) ; le dépôt lui-même n'a pas été consulté.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.