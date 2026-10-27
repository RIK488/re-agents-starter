# simorph_scientific — moteurs scientifiques open source intégrés

Scripts serveurs (protocole JSON-lines, `adapters/PROTOCOL.md`) exécutés par
`simorph-adapter-scientific-python` via `PythonComponent` (`python -I`,
environnement nettoyé, délai, plafond de sortie). Interpréteur : `SIMORPH_PYTHON`,
sinon `simorph/.venv/bin/python` (créé par `scripts/setup-python.sh`).

| Composant | Moteur | Licence |
|---|---|---|
| `scipy.integrate.solve_ivp` | SciPy `solve_ivp` (RK45, DOP853, Radau, BDF, LSODA), Jacobien symbolique SymPy, événements | BSD-3-Clause |
| `scipy.optimize` | `minimize`, `differential_evolution`, `linprog` (HiGHS), gradients symboliques | BSD-3-Clause |
| `scipy.stats.qmc.moments` | Sobol brouillé, Halton, LHS ; erreur-type par réplications | BSD-3-Clause |
| `sympy.dsolve` | `dsolve` + vérification `checkodesol` | BSD-3-Clause |
| `mujoco.simulation` | MuJoCo (pendule, chute libre, chariot-pendule, MJCF utilisateur filtré) | Apache-2.0 |
| `gymnasium.episode` | environnements classic-control (liste blanche) | MIT |
| `pytorch.mlp_regression` | PyTorch CPU, déterministe | BSD-3-Clause |

Sécurité : les expressions (seconds membres, objectifs, contraintes) sont
compilées par `common.compile_expr` (analyse `ast` en liste blanche, jamais
`eval`/`sympify`) ; le MJCF utilisateur refuse `include`, références de
fichiers et plugins ; les valeurs non finies deviennent `null` (JSON).
