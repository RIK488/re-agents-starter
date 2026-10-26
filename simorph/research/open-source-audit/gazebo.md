# Gazebo (gz-sim)

**Catégorie :** Simulation physique et robotique  
**Décision :** adapter (phase 3)  

## 1. Dépôt officiel

https://github.com/gazebosim/gz-sim

## 2. Fonctionnalités pertinentes pour SIMORPH

Simulateur robotique complet : capteurs, monde 3D, plugins physique (DART, Bullet, TPE), transport ; releases Ionic (2024, 2 ans) et Jetty (LTS, support jusqu'en 2031, transport Zenoh).

## 3. Licence et restrictions

Apache-2.0 (connaissance stable ; non vérifié aujourd'hui sur le fichier LICENSE).

## 4. Activité de maintenance

Actif (Jetty LTS 2026).

## 5. Dépendances

C++, Ogre, DART, protobuf, Qt.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

C++/protobuf (gz-transport) : passerelle via subprocess/HTTP ; ROS 2 via ros_gz.

## 7. Besoins CPU/GPU

CPU + GPU pour rendu/capteurs.

## 8. Interfaces disponibles

C++, gz-transport (protobuf), CLI, ROS 2.

## 9. Limites connues

Lourd ; configuration par SDF ; latence de démarrage.

## 10. Adaptateur SIMORPH nécessaire

Container + HTTP/gRPC (passerelle à écrire).

## 11. Tests d'intégration prévus

Chute d'un objet dans un monde vide : trajectoire vs chute libre analytique (±1 %).

## 12. Décision motivée

**Adapter (phase 3, robotique).** Non bloquant.

---

**Dernière vérification :** 2026-10-26 — connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration ; calendrier des releases : vérifié le 2026-10-26 par recherche web.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.