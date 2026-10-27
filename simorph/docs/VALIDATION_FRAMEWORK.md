# SIMORPH — Dispositif de validation scientifique et technique

## 1. Deux questions distinctes

* **Validation technique** : le logiciel fait-il ce qu'il prétend ? (tests,
  couverture, propriétés, reproductibilité, sécurité).
* **Validation scientifique** : un résultat produit par SIMORPH est-il digne
  du niveau de preuve qu'on lui accorde ? (références, convergence,
  incertitude, preuve formelle, comparaison aux observations).

Le nombre de tests, de lignes ou de modules n'est jamais un indicateur de
capacité. Seules comptent les preuves listées ci-dessous.

## 2. Validation technique

| Niveau | Exigence | Outil |
|---|---|---|
| Unitaire | chaque algorithme testé contre une référence **indépendante** (solution analytique, valeur publiée, autre implémentation), jamais contre sa propre sortie | `cargo test` |
| Croisée | chaque moteur natif comparé à une bibliothèque mature quand elle existe (SciPy, MuJoCo, Gymnasium, SymPy) | `adapters/scientific-python` |
| Propriétés | invariants (conservation, bornes, monotonie) sur entrées aléatoires | tests de propriétés |
| Intégration | campagnes de bout en bout sur problèmes de plusieurs secteurs | `tests/integration` |
| Couverture | couverture de lignes mesurée par crate, publiée dans l'audit | `cargo llvm-cov` |
| Sécurité | sondes d'attaque contre le bac à sable (réseau, fichiers, mémoire, processus, secrets) | tests de `simorph-execution`, audit |
| Reproductibilité | même graine ⇒ mêmes hachages de sortie ; reproduction d'expériences archivées | `simorph experiment reproduce` |
| Témoins négatifs | une modification dégradante doit être rejetée ; un faux juge doit être refusé | `services/improvement-loop` |

## 3. Validation scientifique : niveaux de preuve

| Niveau | Condition d'octroi | Qui l'accorde |
|---|---|---|
| Hypothèse non vérifiée | état initial | — |
| Calculé | méthode exacte ou fermée, contrôles passés | `simorph-validation::classify` |
| Simulé | simulation numérique, contrôles passés (invariants, convergence, reproductibilité) | idem |
| Validé empiriquement | comparaison réussie à des **observations** (origine `Empirical`) avec test statistique | idem, références scellées |
| Démontré mathématiquement | certificat formel (SMT, symbolique, Lean) re-vérifié indépendamment | `simorph-formal` + `classify` |

Aucun composant ne peut relever son propre niveau ; le validateur peut
seulement l'abaisser ou le confirmer.

## 4. Contrôles disponibles

Dimensionnel, invariants, Richardson et ordre de convergence, convergence des
résidus, reproductibilité, sensibilité, z-test, χ², couverture des
incertitudes, cas de référence, généralisation, certificats formels.

## 5. Protection du juge

* Références et suites scellées : toute modification après scellement est
  refusée (`Protected`) ; l'empreinte figure dans chaque enregistrement.
* Critères de réussite hachés à la planification ; un résultat évalué avec
  des critères différents est refusé.
* Les agents autonomes n'obtiennent jamais `SelfModify` ni
  `ExternalSideEffect` sans approbation humaine.

## 6. Critère de réussite global

SIMORPH réussit lorsqu'il démontre, avec traçabilité complète :

1. la résolution correcte d'un problème **nouveau** (non présent dans les
   exemples), avec un niveau de preuve justifié ;
2. une amélioration **mesurée et significative** d'un système d'IA sur une
   suite scellée, obtenue à partir de données qu'il a lui-même produites et
   validées, accompagnée du rejet d'un témoin dégradant.

Les démonstrations correspondantes et leurs chiffres figurent dans
`STATUS.md` et dans l'audit.
