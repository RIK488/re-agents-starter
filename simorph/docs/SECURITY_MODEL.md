# SIMORPH — Modèle de sécurité

## Principaux et rôles
* Principaux : `User`, `Service`, `AutonomousAgent`.
* Rôles : `Admin`, `Researcher`, `Operator`, `Viewer`, `Agent`.
* Les permissions (`simorph_core::Permission`) sont demandées par les
  manifestes des composants et accordées par la politique RBAC
  (`simorph-security`). Un composant ne peut jamais s'accorder une permission.

## Invariants (vérifiés par tests)
1. Un `AutonomousAgent` n'obtient jamais `ExternalSideEffect` ni `SelfModify`
   sans un jeton `HumanApproval` (approbateur, périmètre, expiration, empreinte).
2. Le rôle `Admin` n'est jamais attribué automatiquement.
3. Les ressources protégées (`ProtectedGuard`, `ProtectedReferences`) ne
   peuvent être modifiées qu'avec Admin + approbation humaine ; toute
   altération hors circuit est détectée par empreinte.
4. Chaque décision d'autorisation est journalisée (journal d'audit en
   ajout seul).
5. Les secrets ne sont lus que depuis une liste blanche de variables
   d'environnement et sont masqués dans les journaux.

## Isolation de l'exécution
| Niveau | Mécanisme | État |
|---|---|---|
| Composant Rust natif | In-process, `spawn_blocking`, délai, annulation coopérative | implémenté |
| Composant externe / code généré | Sous-processus, protocole JSON-lines, délai avec arrêt forcé, plafond de sortie, environnement nettoyé (liste blanche), répertoire de travail dédié | implémenté |
| Isolation forte | Conteneur (`ExecutionInterface::Container`) avec restrictions réseau, cgroups, système de fichiers en lecture seule | manifestes `infrastructure/`, exécution déléguée au runtime de conteneurs |
| Isolation déterministe | WebAssembly (Wasmtime, WASI) | planifié (ADR-0001) |

## Auto-amélioration
Toute modification proposée par SIMORPH sur lui-même suit : proposition →
bac à sable → évaluation indépendante (suites protégées) → décision humaine
pour `SelfModify` → déploiement → possibilité de retour arrière. Les
références protégées et la politique de sécurité ne sont pas dans le périmètre
modifiable.
