# Bancs de référence

Les bancs mesurent précision et coût des moteurs natifs contre des solutions
analytiques ou des références scellées. Ils sont exécutés par
`cargo run -p simorph-cli -- bench` et produisent un rapport JSON
reproductible (seed, versions, environnement) archivé dans la provenance.

| Banc | Référence | Métrique |
|---|---|---|
| Intégrateurs (RK4, RK45, implicites) | oscillateur harmonique, Kepler | erreur d'énergie, ordre de convergence, temps |
| Cosimulation | modèle monolithique équivalent | erreur vs pas macro |
| Sobol | fonction d'Ishigami | écart aux indices analytiques |
| Optimisation | Rosenbrock, Rastrigin, ZDT1, g06 | évaluations jusqu'à tolérance, hypervolume |
