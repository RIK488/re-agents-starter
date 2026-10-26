# SIMORPH — Registre des exigences

Chaque exigence a un identifiant stable `REQ-<sous-système>-<n>`. Les critères
d'acceptation correspondants sont dans `ACCEPTANCE_CRITERIA.md` ; l'état réel
(implémenté et testé / partiel / planifié / recherche) est dans `STATUS.md`.
Les exigences couvrent **le système final complet**, conformément à la règle de
non-réduction du cahier des charges.

## Interface universelle (UI)
- REQ-UI-1 Décrire un problème en langage naturel et sous forme structurée (`ProblemSpec`).
- REQ-UI-2 Joindre documents, images, données, équations, fichiers techniques avec empreinte SHA-256.
- REQ-UI-3 Définir objectifs, contraintes, variables de conception, budget, poids du compromis.
- REQ-UI-4 Visualiser les hypothèses et leur statut.
- REQ-UI-5 Inspecter les modèles générés (descripteurs, équations, unités, validité).
- REQ-UI-6 Configurer et lancer des simulations ; suivre les expériences en direct (flux de progression).
- REQ-UI-7 Comparer les solutions (fronts de Pareto, tableaux de compromis).
- REQ-UI-8 Visualiser les résultats en 2D et 3D.
- REQ-UI-9 Examiner les incertitudes (distributions, indices de sensibilité).
- REQ-UI-10 Accéder aux preuves, enregistrements de validation et journaux d'exécution.
- REQ-UI-11 Relancer ou modifier une expérience ; reproduire par identifiant.
- REQ-UI-12 Exporter modèles, rapports, codes et résultats.
- REQ-UI-13 Chaque commande visible correspond à une action réellement implémentée et testée ; les actions non disponibles sont absentes de l'interface ou explicitement marquées « non implémenté ».

## Directeur scientifique (DIR)
- REQ-DIR-1 Décomposer un problème en sous-problèmes typés (connu / inconnu / calculable / à modéliser / à simuler / données manquantes / à démontrer / à vérifier).
- REQ-DIR-2 Identifier les disciplines concernées à partir du problème et du registre.
- REQ-DIR-3 Formuler des hypothèses avec critère de réfutation.
- REQ-DIR-4 Établir un programme expérimental (plans ordonnés, budget, fidélité).
- REQ-DIR-5 Sélectionner les simulateurs/composants par capacités, domaines et compatibilité d'unités.
- REQ-DIR-6 Allouer des ressources (via l'ordonnanceur) et respecter le budget.
- REQ-DIR-7 Définir des critères de réussite **avant** exécution et les enregistrer.
- REQ-DIR-8 Réviser les hypothèses selon les résultats validés.
- REQ-DIR-9 Détecter les contradictions entre résultats.
- REQ-DIR-10 Décider des expériences suivantes par valeur informative / compromis.
- REQ-DIR-11 Arrêter les recherches improductives (règles d'arrêt mesurables).
- REQ-DIR-12 Produire des conclusions traçables (chaque conclusion liée à ses runs et validations).
- REQ-DIR-13 Le directeur ne peut pas modifier les critères indépendants (références scellées, poids du compromis, suites d'évaluation protégées).
- REQ-DIR-14 Le raisonnement LLM est optionnel, remplaçable, et ne peut pas promouvoir un niveau de preuve.

## Modélisation (MOD)
- REQ-MOD-1 Représenter les formalismes : EDO, systèmes algébriques, contraintes logiques, modèles probabilistes, systèmes dynamiques, graphes, géométries, multiagents, processus discrets, modèles continus, hybrides, fondés sur les données, physiques/numériques.
- REQ-MOD-2 Chaque modèle déclare entrées/sorties, unités, hypothèses, domaine de validité, paramètres, dépendances, incertitudes, critères de vérification.
- REQ-MOD-3 Vérification dimensionnelle automatique des connexions et des équations.
- REQ-MOD-4 Chaînage automatique de composants pour produire les sorties requises à partir des entrées disponibles.

## Génération de simulateurs (GEN)
- REQ-GEN-1..10 Les dix étapes de §3.4 (recherche, compatibilité, configuration, intégration, complément, génération, test, comparaison de méthodes, amélioration, documentation des limites).
- REQ-GEN-11 Un simulateur généré n'est jamais « validé » sans confrontation à des références appropriées ; son niveau de preuve initial est `Simulated` au plus.
- REQ-GEN-12 Le code généré s'exécute uniquement en bac à sable.

## Cosimulation (COS)
- REQ-COS-1 Synchronisation temporelle multi-esclaves (Jacobi, Gauss-Seidel, itératif).
- REQ-COS-2 Échanges de variables typés avec conversion d'unités automatique.
- REQ-COS-3 Graphe de dépendances, détection de boucles algébriques.
- REQ-COS-4 Conditions initiales cohérentes ; vérification avant démarrage.
- REQ-COS-5 Contrôle de stabilité numérique (pas macro adaptatif, estimation d'erreur).
- REQ-COS-6 Propagation des incertitudes par ensembles.
- REQ-COS-7 Détection d'incohérences (NaN, sortie de domaine, plages de ports) avec identification de l'esclave et de l'instant.
- REQ-COS-8 Esclaves FMI 2.0/3.0 via adaptateur.

## Expérimentation (EXP)
- REQ-EXP-1 Plans d'expériences (factoriel, LHS, aléatoire, OAT, composite centré).
- REQ-EXP-2 Exploration de paramètres parallèle et déterministe.
- REQ-EXP-3 Comparaison d'architectures / de méthodes sur le même plan.
- REQ-EXP-4 Analyses de sensibilité (locale, globale).
- REQ-EXP-5 Recherche de contre-exemples.
- REQ-EXP-6 Identification des causes probables d'un échec.
- REQ-EXP-7 Reproduction d'une expérience par identifiant avec vérification des hachages.
- REQ-EXP-8 Archivage et provenance complète par expérience et par run.

## Optimisation (OPT)
- REQ-OPT-1 Familles : classique, sous contraintes, bayésienne, multiobjectif, évolutionnaire, arborescente, renforcement, heuristique, différentiable, robuste, guidée par l'incertitude.
- REQ-OPT-2 Sélection motivée de la méthode selon les traits du problème.
- REQ-OPT-3 Tout optimiseur honore annulation et budget.

## Validation (VAL)
- REQ-VAL-1..11 Contrôles : cohérence dimensionnelle, contraintes physiques, erreurs numériques, convergence, reproductibilité, sensibilité, robustesse, généralisation, cohérence statistique, comparaison aux références, validité des conclusions.
- REQ-VAL-12 Distinction stricte des cinq catégories épistémiques ; transitions uniquement par enregistrement de validation.
- REQ-VAL-13 Références scellées, infalsifiables sans Admin + approbation humaine ; empreinte reportée dans chaque enregistrement.
- REQ-VAL-14 La validation est indépendante du directeur (aucune dépendance de code).

## Incertitude (UNC)
- REQ-UNC-1 Évaluer les incertitudes (propagation Monte Carlo, LHS, quasi-aléatoire).
- REQ-UNC-2 Identifier les paramètres dominants (Sobol, Morris).
- REQ-UNC-3 Estimer la valeur informative d'une expérience (EVPI / réduction de variance attendue).
- REQ-UNC-4 Choisir le niveau de fidélité nécessaire ; éviter les simulations inutilement coûteuses ; augmenter progressivement la précision.
- REQ-UNC-5 Identifier les situations nécessitant des données réelles.
- REQ-UNC-6 Comparer prédictions et observations ; réviser les modèles (calibration).
- REQ-UNC-7 Compromis précision/temps/coût/risque/valeur informative mesurable et journalisé.

## Noyau Rust (RUN)
- REQ-RUN-1 Orchestration de graphes de tâches avec dépendances de données.
- REQ-RUN-2 Exécution concurrente, admission par ressources (CPU, mémoire, GPU).
- REQ-RUN-3 Gestion des processus externes (sous-processus, protocole JSON-lines, conteneurs).
- REQ-RUN-4 Délais, annulation, réessais, isolation des pannes.
- REQ-RUN-5 Reprise après interruption (snapshot/resume).
- REQ-RUN-6 Métriques et traces (Prometheus, OpenTelemetry).
- REQ-RUN-7 Gestion de la provenance intégrée à l'exécution.
- REQ-RUN-8 Exécution distribuée multi-machines / multi-GPU (adaptateur Ray / Kubernetes).

## Sécurité (SEC)
- REQ-SEC-1 Authentification et autorisations par rôle.
- REQ-SEC-2 Isolation des expériences et du code généré (bac à sable, conteneurs).
- REQ-SEC-3 Gestion des secrets et masquage dans les journaux.
- REQ-SEC-4 Restrictions réseau par politique.
- REQ-SEC-5 Journalisation d'audit infalsifiable.
- REQ-SEC-6 Contrôle des ressources.
- REQ-SEC-7 Protection contre les modifications non autorisées (références, politiques, code).
- REQ-SEC-8 Validation avant déploiement d'une modification autonome.
- REQ-SEC-9 Aucun privilège administrateur ni action physique/externe sensible pour un agent autonome sans approbation humaine.

## Laboratoire IA (AIL)
- REQ-AIL-1 Boucle complète §5.1 (évaluation → limites → hypothèses → expériences → exécution → validation indépendante → connaissances → données → entraînement → réévaluation → acceptation/rejet → itération).
- REQ-AIL-2 Cibles : modèles de langage, multimodaux, vision, contrôle, agents, planification, recherche, prédictifs, embarqués, scientifiques.
- REQ-AIL-3 Acceptation uniquement sur amélioration significative sans régression sur métriques protégées.
- REQ-AIL-4 Vitesse et raisonnement sont des métriques distinctes, jamais confondues.
- REQ-AIL-5 Auto-amélioration de SIMORPH en environnement isolé avec évaluation indépendante et retour arrière.
- REQ-AIL-6 Aucune modification autonome ne peut neutraliser les contrôles de sécurité ni altérer les références protégées.

## Intégration open source (OSS)
- REQ-OSS-1 Fiche d'audit à 12 champs pour chaque projet candidat.
- REQ-OSS-2 Décision motivée intégrer / adapter / remplacer / écarter.
- REQ-OSS-3 Aucun composant mature réécrit sans raison technique démontrée.

## Mémoire et traçabilité (MEM)
- REQ-MEM-1 Mémoire persistante distinguant connaissances établies, hypothèses, expériences, modèles, simulateurs, jeux de données, résultats, échecs, corrections, décisions d'architecture, améliorations mesurées.
- REQ-MEM-2 Chaque résultat lié à problème, données, hypothèses, versions logicielles, paramètres, environnement, métriques, validations.
- REQ-MEM-3 Expliquer pourquoi une solution a été retenue et reproduire le processus.

## Transfert interdisciplinaire (TRF)
- REQ-TRF-1 Représentation des connaissances reliant des concepts de disciplines différentes.
- REQ-TRF-2 Les transferts sont des hypothèses à vérifier ; une analogie n'est jamais une preuve.
