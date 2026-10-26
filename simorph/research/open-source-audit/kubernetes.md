# Kubernetes

**Catégorie :** Infrastructure  
**Décision :** intégrer  

## 1. Dépôt officiel

https://github.com/kubernetes/kubernetes

## 2. Fonctionnalités pertinentes pour SIMORPH

Orchestration de conteneurs, jobs, GPU scheduling (device plugins), secrets, autoscaling.

## 3. Licence et restrictions

Apache-2.0.

## 4. Activité de maintenance

Très actif.

## 5. Dépendances

Conteneurs OCI.

## 6. Compatibilité technique (noyau Rust / pont Python / FMI)

Crate Rust `kube` pour piloter des Jobs depuis simorph-orchestrator ; manifests dans infrastructure/.

## 7. Besoins CPU/GPU

Cluster.

## 8. Interfaces disponibles

API REST, kubectl, Helm.

## 9. Limites connues

Complexité opérationnelle.

## 10. Adaptateur SIMORPH nécessaire

HTTP (kube-rs).

## 11. Tests d'intégration prévus

Lancer un Job exécutant un composant conteneurisé et récupérer le TaskResult par le protocole.

## 12. Décision motivée

**Intégrer (déploiement distribué §10.2-10.3).**

---

**Dernière vérification :** 2026-10-26 — connaissance antérieure (non revérifiée aujourd'hui) — à confirmer sur le fichier LICENSE du dépôt avant intégration.

Les faits marqués « vérifié le 2026-10-26 » proviennent de sources consultées ce jour (métadonnées PyPI/crates.io, pages officielles ou recherche web citée) ; les faits marqués « connaissance antérieure » doivent être revérifiés sur le fichier LICENSE et l'historique du dépôt avant toute intégration.