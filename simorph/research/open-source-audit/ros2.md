# ROS 2

**Catégorie :** Simulation physique et robotique  
**Décision :** adapter (phase 3)  

## 1. Dépôt officiel

https://github.com/ros2/ros2

## 2. Fonctionnalités pertinentes pour SIMORPH

Middleware robotique (DDS/Zenoh), outils, ros2_control, navigation ; distributions Kilted Kaiju (mai 2025) et Lyrical Luth (mai 2026).

## 3. Licence et restrictions

Apache-2.0 pour le cœur (connaissance stable) ; paquets tiers sous licences variées (BSD, GPL pour certains).

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

DDS (Fast DDS/Cyclone), colcon, Ubuntu LTS.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Bindings Rust existants (rclrs, ros2-client) ; ros2 via Zenoh bridge ; non nécessaire pour la simulation pure.

## 7. Besoins CPU/GPU

CPU.

## 8. Interfaces disponibles

C++, Python, Rust (communautaire), CLI.

## 9. Limites connues

Système de build lourd ; versionnage par distribution Ubuntu.

## 10. Adaptateur SIMORPH nécessaire

HTTP/gRPC passerelle ou Rust rclrs (phase 3).

## 11. Tests d'intégration prévus

Boucle de commande PD de plugins/physics (attitude) publiée/consommée via rclrs sur une démo locale.

## 12. Décision motivée

**Adapter (phase 3).** Nécessaire uniquement pour les cas robotiques connectés au matériel ; les actions physiques restent soumises à `ExternalSideEffect` (§10.4).

---

**Dernière vérification :** 2026-10-26 — connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration ; distributions : vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.