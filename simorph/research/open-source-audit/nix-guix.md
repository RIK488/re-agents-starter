# Nix / GNU Guix (environnements reproductibles)

**Catégorie :** Complémentaire : reproductibilité  
**Décision :** intégrer (infra)  
**Statut :** candidat complémentaire proposé par Claude (non listé dans le cahier des charges §6)  

## 1. Dépôt officiel

https://github.com/NixOS/nix ; https://git.savannah.gnu.org/git/guix.git

## 2. Fonctionnalités pertinentes pour SIMORPH

Gestion de paquets fonctionnelle et environnements reproductibles bit à bit ; Nix utilisé pour reproduire l'écosystème preCICE (étude Univ. Stuttgart) ; Guix pour les workflows de recherche.

## 3. Licence et restrictions

Nix : LGPL-2.1 ; Nixpkgs : MIT ; Guix : GPL-3.0 (connaissance stable, non vérifié aujourd'hui).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Linux/macOS.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Outil d'environnement : fournit l'`EnvironmentCapture` de simorph-provenance (hash de closure) ; aucune liaison de code.

## 7. Besoins CPU/GPU

Disque.

## 8. Interfaces disponibles

CLI, flakes.

## 9. Limites connues

Courbe d'apprentissage ; GPU/CUDA plus délicats sous Nix.

## 10. Adaptateur SIMORPH nécessaire

Aucun (infrastructure/).

## 11. Tests d'intégration prévus

Reconstruire l'environnement Python des adaptateurs depuis un flake et vérifier l'égalité des hash de sorties d'une expérience.

## 12. Décision motivée

**Intégrer (infrastructure, Nix).** Reproductibilité des environnements (§3.6, §11).

---

**Dernière vérification :** 2026-10-26 — vérifié le 2026-10-26 par recherche web (Nix) ; Guix : connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.