# Protocole SIMORPH de composant hors processus (JSON Lines) — version 1.0

Ce protocole relie le noyau SIMORPH (`simorph-execution::SubprocessComponent`)
à un composant exécuté dans un processus enfant : script Python
(`adapters/python`), pilote FMI (`adapters/fmi`), composant Rust isolé
(`simorph_plugin_sdk::serve_stdio`), ou tout autre programme.

Les types de messages sont définis dans
`crates/simorph-plugin-sdk/src/protocol.rs` (`Request`, `Response`) ; cette
page en est la spécification normative.

## Transport

* Canal : `stdin` du processus enfant pour les requêtes, `stdout` pour les
  réponses. `stderr` est libre (journaux), jamais interprété.
* Encodage : UTF-8, **un objet JSON par ligne**, terminé par `\n`.
* Le champ `type` discrimine les messages (`snake_case`).
* Le composant ne doit **rien** écrire d'autre sur `stdout`. Toute ligne non
  vide qui n'est pas un message valide est une violation de protocole : le
  processus est tué et relancé (dans la limite de `max_restarts`).
* Le processus reste vivant entre deux exécutions (interpréteur chaud). Il
  doit traiter les requêtes séquentiellement.

## Requêtes (noyau → composant)

| `type` | Champs | Réponse attendue |
|---|---|---|
| `manifest` | — | `manifest` |
| `execute` | `request` : `TaskRequest` | zéro ou plusieurs `progress`, puis `result` **ou** `error` |
| `cancel` | — | `ack` (annulation au mieux ; le noyau tue de toute façon le processus) |
| `shutdown` | — | `ack`, puis le processus se termine |

### `TaskRequest`

```json
{
  "run_id": "uuid", "component": "uuid",
  "inputs": {"nom": valeur, ...},
  "seed": 42, "fidelity": 1.0, "config": null
}
```

* `inputs` suit `input_schema` du manifeste (valeurs JSON : nombre, tableau…).
* `seed` : graine pour un résultat reproductible (`null` = défaut du composant).
* `fidelity` ∈ [0, 1] : niveau de fidélité demandé par la simulation guidée par
  l'incertitude (§8). `0` = approximation la moins coûteuse.
* `config` : configuration libre comprise par le composant.

## Réponses (composant → noyau)

| `type` | Champs |
|---|---|
| `manifest` | `manifest` : `ComponentManifest` complet (voir `schemas/`) |
| `result` | `result` : `TaskResult` |
| `error` | `message` (chaîne), `run_id` (optionnel) |
| `progress` | `fraction` ∈ [0, 1], `message` (optionnel) — ignoré fonctionnellement, journalisé |
| `ack` | — |

### `TaskResult`

```json
{
  "run_id": "uuid", "component": "uuid",
  "outputs": {"nom": valeur, ...},
  "metrics": {"nom": nombre, ...},
  "evidence": "computed" | "simulated" | "empirically_validated" | "mathematically_proven" | "unverified_hypothesis",
  "request_hash": "", "warnings": ["..."], "wall_time_ms": 0
}
```

Le noyau **écrase** `run_id`, `component` et `request_hash` avec ses propres
valeurs : le composant n'est pas l'autorité sur l'identité de l'exécution.
Le niveau `evidence` est une **déclaration** du composant ; le moteur de
validation peut l'abaisser, jamais l'augmenter sur la seule foi du composant.

## Contraintes de bac à sable appliquées par le noyau

* Environnement vidé puis restreint à `SandboxPolicy.env_allowlist`.
* Répertoire de travail imposé (`working_dir`).
* Délai `max_cpu_s` (mur) : dépassement ⇒ processus tué ⇒ `Timeout`.
* Plafond `max_output_bytes` sur `stdout` : dépassement ⇒ processus tué.
* Annulation coopérative : `cancel` envoyé puis processus tué ⇒ `Cancelled`.
* Isolation réseau / fichiers / mémoire : couche conteneur
  (`SandboxPolicy::to_container_args`), non garantie par le protocole seul.

## Exemple de session

```
→ {"type":"manifest"}
← {"type":"manifest","manifest":{...}}
→ {"type":"execute","request":{"run_id":"…","component":"…","inputs":{"x":3.0},"seed":null,"fidelity":1.0,"config":null}}
← {"type":"progress","fraction":0.5}
← {"type":"result","result":{"run_id":"…","component":"…","outputs":{"y":9.0},"metrics":{},"evidence":"computed","request_hash":"","warnings":[],"wall_time_ms":1}}
→ {"type":"shutdown"}
← {"type":"ack"}
```

## Versionnage

`PROTOCOL_VERSION = "1.0"`. Les ajouts de champs optionnels sont compatibles ;
tout changement de sémantique incrémente la version majeure et le manifeste
devra la déclarer (champ à ajouter dans `ComponentProvenance` ou
`ExecutionInterface`).
