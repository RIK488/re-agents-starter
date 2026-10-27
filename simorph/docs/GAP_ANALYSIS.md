# SIMORPH — Analyse des capacités manquantes

État au 2026-10-27, après l'audit et les travaux de cette livraison.
Échelle : **Opérationnel** (démontré par tests et mesures dans cet
environnement), **Expérimental** (fonctionne dans un périmètre restreint ou
sur des cibles réduites), **Absent**, **Recherche** (capacité non acquise
scientifiquement, voir `RESEARCH_REQUIREMENTS.md`).

## 1. Par sous-système

| # | Sous-système | Niveau atteint | Manque principal | Nature du manque |
|---|---|---|---|---|
| 1 | Noyau Rust | Opérationnel | ordonnanceur et orchestrateur encore hors du chemin des campagnes (le runtime exécute directement) | ingénierie |
| 2 | Multi-agents | Expérimental | raisonnement LLM réel (client prêt, testé sur serveur factice ; aucune clé ici) | accès à un modèle |
| 3 | Compréhension | Expérimental | analyse par lexiques et règles ; phrases hors lexique signalées mais non comprises | ingénierie + LLM |
| 4 | Modèles | Opérationnel (EDO explicites, algébrique, lois découvertes) | EDP, DAE, Modelica (OpenModelica bloqué ici) | intégration |
| 5 | Simulateurs | Expérimental | génération de physiques nouvelles sans équations fournies | **recherche (RR-1)** |
| 6 | Cosimulation | Opérationnel (natif) ; FMU partiel | exécution de FMU (FMPy non installé), preCICE | intégration |
| 7 | Multimodal | Opérationnel (analyses signal/géométrie) | modèles de vision, audio et vidéo appris | intégration + GPU |
| 8 | Calcul scientifique | Opérationnel (natif + SciPy, SymPy, MuJoCo) | EDP (FEniCSx, PETSc), Chrono, Warp GPU | intégration + GPU |
| 9 | Expérimentation autonome | Opérationnel sur problèmes à composants existants | sélection d'expériences par valeur d'information réelle ; résolution de contradictions ; plans de contre-exemples automatiques | ingénierie + **recherche (RR-6)** |
| 10 | Évolution | Opérationnel (GP dimensionnelle, MAP-Elites, évolution de code par mutations syntaxiques) | mutations par LLM en conditions réelles | accès à un modèle |
| 11 | Entraînement | Opérationnel (MLP natif, PyTorch CPU) | fine-tuning de LLM (TRL/PEFT), GPU, distribué | matériel |
| 12 | Renforcement | Opérationnel (PPO, REINFORCE, Gymnasium, MuJoCo) | RL distribué (RLlib), environnements lourds | matériel + intégration |
| 13 | Formel | Opérationnel (Z3 certifié, SymPy, intervalles) | Lean 4 non installé ; fonctions transcendantes en SMT ; lien preuve ↔ simulation flottante | intégration + **recherche (RR-3)** |
| 14 | Évaluation | Expérimental | harnais externes (Inspect AI, lm-eval) non installés ; modèles de langage absents | intégration + matériel |
| 15 | Mémoire | Opérationnel (local) | PostgreSQL/Qdrant multi-utilisateur ; analogies interdisciplinaires produites automatiquement | ingénierie |
| 16 | Distribué | Opérationnel (coordinateur/workers TCP, reprise sur panne) | GPU, Ray, Jobs Kubernetes, chiffrement du transport | matériel + intégration |
| 17 | Exécution sûre | Opérationnel (réseau, mémoire, CPU, fichiers, groupe de processus) | restriction du système de fichiers (montages), limite de processus (cgroups) | conteneurs |
| 18 | Interface | Opérationnel | authentification OIDC, multi-utilisateur, visualisation ParaView | ingénierie |
| 19 | Connecteurs | Opérationnel (CSV, JSON-lines, HTTP, rejeu, SCPI sur transport) | instruments physiques réels, réglages de ligne série | accès matériel |
| 20 | Amélioration continue | **Opérationnel sur surrogates de simulateurs** (gains mesurés, témoins rejetés) | application à des modèles de langage ; amélioration de SIMORPH lui-même au niveau du code | matériel + **recherche (RR-2, RR-8)** |

## 2. Manques transverses

1. **Accès à un modèle de langage.** Les clients (raisonneur, interprète,
   mutateur d'évolution) existent et sont testés contre un serveur factice ;
   aucune clé n'est configurée. Effet : la compréhension et l'évolution de
   code reposent sur des règles et des mutations syntaxiques.
2. **Matériel.** Aucun GPU ni cluster : entraînement de grands modèles,
   simulation GPU et distribution multi-nœuds réelle ne sont pas exercés.
3. **Données réelles.** Aucune observation expérimentale réelle n'est
   fournie ; la calibration et le niveau « validé empiriquement » sont
   démontrés sur des observations synthétiques étiquetées comme telles.
4. **Heuristiques de pilotage.** Le « gain informatif » du directeur reste
   une utilité heuristique ; elle est documentée comme telle.
5. **Couche de confiance en mode sans clé.** Provenance et sceaux détectent
   les modifications accidentelles ; contre un attaquant ayant accès au
   disque, il faut le mode à clé (HMAC).

## 3. Ce qui ne doit pas être confondu

| Affirmation | Réalité |
|---|---|
| « SIMORPH améliore les IA » | démontré pour des réseaux de substitution de simulateurs et des politiques RL ; non démontré pour des modèles de langage |
| « SIMORPH génère des simulateurs » | il réutilise, assemble, transcrit des équations et génère du code à partir d'équations ; il ne découvre pas une physique inconnue sans données |
| « SIMORPH découvre des lois » | démontré sur des lois à quelques variables (Kepler, pendule, traînée) avec données propres ; pas sur des systèmes à grande dimension |
| « Résultat démontré » | uniquement des énoncés bornés certifiés par SMT et re-vérifiés ; aucune preuve sur des trajectoires simulées |
