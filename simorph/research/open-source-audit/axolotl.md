# Axolotl

**Catégorie :** Entraînement des IA  
**Décision :** écarter / option  

## 1. Dépôt officiel

https://github.com/axolotl-ai-cloud/axolotl

## 2. Fonctionnalités pertinentes pour SIMORPH

Fine-tuning par fichiers YAML (SFT, DPO, LoRA/QLoRA), multi-GPU, datasets.

## 3. Licence et restrictions

Apache-2.0 (connaissance stable ; champ licence PyPI `axolotl` 0.21.0 vide — vérifier le dépôt).

## 4. Activité de maintenance

Actif (0.21.0, 2026-10-16).

## 5. Dépendances

Transformers, PEFT, TRL, DeepSpeed.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

CLI + YAML : subprocess en conteneur.

## 7. Besoins CPU/GPU

GPU.

## 8. Interfaces disponibles

CLI, YAML.

## 9. Limites connues

Surcouche ; moins de contrôle fin.

## 10. Adaptateur SIMORPH nécessaire

Container.

## 11. Tests d'intégration prévus

Config YAML LoRA minimal reproduisant le test PEFT.

## 12. Décision motivée

**Écarter (redondant avec TRL/PEFT).** Option de confort.

---

**Dernière vérification :** 2026-10-26 — PyPI vérifié (champ vide) ; licence : connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.