# SIMORPH — Critères d'acceptation

Un critère est **satisfait** uniquement s'il existe un test automatisé (ou un
protocole reproductible documenté) qui le vérifie, référencé dans `STATUS.md`.
Un composant non testé n'est pas déclaré opérationnel.

| ID | Critère | Vérification attendue |
|---|---|---|
| AC-CORE-1 | Les cinq niveaux de preuve ne sont pas interchangeables | Test : promotion `Simulated → MathematicallyProven` refusée ; promotion sans enregistrement de validation impossible. |
| AC-CORE-2 | Tout manifeste déclare ports, unités, hypothèses, validité, incertitude, précision, déterminisme, ressources, dépendances, interface, protocole de validation, provenance, permissions | Test : désérialisation du schéma JSON ; port sans unité rejeté. |
| AC-UNITS-1 | Conversion exacte entre unités compatibles, erreur entre incompatibles | Tests : km→m, °C→K, kWh→J, eV→J, m→s rejeté. |
| AC-SIM-1 | Intégrateurs vérifiés contre solutions analytiques | Tests : décroissance exponentielle, oscillateur harmonique, ordre de convergence mesuré (RK4≈4, RK45≈5), système raide stable en implicite. |
| AC-SIM-2 | Événements hybrides | Test : balle rebondissante, instants d'impact vs formule analytique. |
| AC-SIM-3 | Annulation et délais honorés | Test : un run long s'interrompt avec `Cancelled`/`Timeout`. |
| AC-COS-1 | Cosimulation équivalente au modèle monolithique | Test : erreur Jacobi/Gauss-Seidel vs monolithique décroît avec le pas macro. |
| AC-COS-2 | Conversion d'unités automatique et rejet des incompatibilités | Tests : kW→W inséré automatiquement ; W→K rejeté à la construction. |
| AC-COS-3 | Boucle algébrique détectée puis résolue par itération | Test dédié. |
| AC-EXP-1 | Plans d'expériences corrects | Tests : tailles, stratification LHS. |
| AC-EXP-2 | Reproduction par identifiant | Test : `reproduce()` renvoie égalité des hachages de sortie. |
| AC-EXP-3 | Contre-exemple et cause d'échec | Tests : violation trouvée ; facteur responsable classé premier. |
| AC-OPT-1 | Chaque méthode atteint l'optimum connu sur ses bancs | Tests : sphère, Rosenbrock, Rastrigin, ZDT1 (hypervolume), problème contraint, sac à dos (MCTS). |
| AC-OPT-2 | Sélection de méthode motivée | Test : classement attendu selon les traits du problème. |
| AC-UNC-1 | Indices de Sobol conformes à l'analytique (Ishigami) | Tolérance ≈ 0,05 à N≈4096. |
| AC-UNC-2 | Valeur de l'information | Test : EVPI d'un problème linéaire vs analytique. |
| AC-UNC-3 | Sélection de fidélité | Test : niveau le moins cher satisfaisant la précision cible. |
| AC-VAL-1 | Onze contrôles opérationnels | Un test par contrôle avec cas analytique. |
| AC-VAL-2 | Références scellées | Test : mutation après scellement → `Protected`. |
| AC-VAL-3 | Jamais `MathematicallyProven` sans artefact de preuve | Test de classification. |
| AC-PROV-1 | Chaîne de provenance infalsifiable | Test : modification d'un enregistrement détectée par `verify_chain()`. |
| AC-PROV-2 | `explain(id)` reconstruit la chaîne causale complète | Test. |
| AC-RUN-1 | Ordonnancement respectant dépendances, ressources, priorités | Tests : concurrence ≤ capacité ; ordre topologique. |
| AC-RUN-2 | Délais, réessais, annulation en cascade, reprise | Tests dédiés. |
| AC-RUN-3 | Composant sous-processus isolé | Tests : script Python exécuté ; délai tue le processus ; sortie plafonnée ; environnement nettoyé. |
| AC-SEC-1 | Agent autonome jamais `ExternalSideEffect`/`SelfModify` sans approbation humaine | Test. |
| AC-SEC-2 | Secrets masqués dans les journaux | Test. |
| AC-SEC-3 | Journal d'audit de chaque décision d'autorisation | Test. |
| AC-DIR-1 | Décomposition du problème en catégories §2 | Test : problème de nanosatellite décomposé, chaque catégorie non vide ou justifiée. |
| AC-DIR-2 | Programme expérimental avec critères de réussite préalables | Test : critères enregistrés dans la provenance avant tout run. |
| AC-DIR-3 | Règles d'arrêt | Test : campagne s'arrête sur budget et sur gain informatif nul. |
| AC-DIR-4 | Conclusions traçables | Test : chaque conclusion référence runs et validations. |
| AC-AIL-1 | Boucle d'amélioration complète sur un candidat jouet | Test : candidat amélioré accepté, candidat régressif rejeté, retour arrière effectif. |
| AC-AIL-2 | Suites d'évaluation protégées | Test : modification par un agent refusée. |
| AC-AIL-3 | Vitesse ≠ raisonnement | Test : un candidat plus rapide mais moins exact est rejeté. |
| AC-GEN-1 | Pipeline de génération de simulateur en 10 étapes | Test : génération d'un composant Python à partir d'un descripteur, exécution en bac à sable, comparaison de solveurs, manifeste documentant les limites. |
| AC-KNW-1 | Transfert interdisciplinaire = hypothèse | Test : suggestion typée `Hypothesis` avec critère de réfutation. |
| AC-UI-1 | Chaque commande visible est implémentée | Test : la liste des commandes de l'UI est générée depuis le registre d'actions testées ; test d'intégration HTTP par route. |
| AC-E2E-1 | Démonstration nanosatellite | Test d'intégration : orbite + éclipse + solaire + batterie + thermique + attitude en cosimulation, bilan d'énergie cohérent, validation et provenance complètes, rapport exporté. |
