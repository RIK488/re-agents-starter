# ADR-0002 — Déterminisme des sérialisations JSON pour la provenance

Statut : accepté. Date : 2026-10-27.

## Contexte
La chaîne de provenance hache la sérialisation JSON de chaque enregistrement.
Lors du rechargement d'un registre JSONL produit par une campagne réelle, un
flottant (`0.10726564416967245`) a été relu avec une erreur d'un ULP, cassant
le hachage et donc la vérification d'intégrité. Cause : l'analyseur rapide de
`serde_json` n'est pas exactement réversible sans la fonctionnalité
`float_roundtrip`.

## Décision
* `serde_json` est compilé avec `float_roundtrip` dans tout le workspace.
* Aucun contrat sérialisé ne contient d'infini ou de NaN (JSON ne les porte
  pas) : les bornes « illimitées » utilisent `f64::MAX` (`Budget::max_cost`,
  plages de ports).
* Les cartes sont ordonnées (`IndexMap`/`BTreeMap`) pour un encodage stable.
* Un test de round-trip existe pour `ProblemSpec` et pour le registre JSONL.

## Conséquences
Le coût d'analyse des flottants augmente légèrement ; la reproductibilité
bit-à-bit des enregistrements est garantie, condition nécessaire à AC-PROV-1.
