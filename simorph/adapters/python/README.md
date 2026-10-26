# simorph_adapter — composants SIMORPH en Python

Ce paquet permet d'exposer n'importe quel code Python (NumPy, SciPy, JAX,
PyTorch, FMPy, Mesa, …) comme **composant SIMORPH** conforme au contrat
universel (`ComponentManifest`, §7 du cahier des charges), exécuté hors
processus par le noyau Rust (`SubprocessComponent`) sous la politique de
bac à sable (`SandboxPolicy`).

## Principe

1. Sous-classer `simorph_adapter.Component` ;
2. renvoyer un `Manifest` (ports d'entrée/sortie avec **unités**, hypothèses,
   domaine de validité, cas de référence) ;
3. implémenter `execute(request) -> Result` ;
4. appeler `serve(component)` dans `__main__`.

```python
from simorph_adapter import Component, Manifest, Port, Request, Result, serve

class Square(Component):
    def manifest(self):
        return Manifest("demo.square", "1.0.0", "moi",
                        inputs=[Port("x", "m")], outputs=[Port("y", "m^2")])

    def execute(self, request: Request) -> Result:
        x = request.f("x")
        return Result(request, evidence="computed").output("y", x * x)

if __name__ == "__main__":
    serve(Square())
```

Côté Rust :

```rust
let c = PythonComponent::from_script("mon_composant.py", SandboxPolicy::strict())?;
```

## Garanties et limites

* L'interpréteur est lancé avec `python3 -I` : pas de `PYTHONPATH`, pas de
  site utilisateur, répertoire du script absent de `sys.path`. Les exemples
  ajoutent explicitement `adapters/python` à `sys.path`.
* L'environnement est **filtré** : seules les variables de
  `SandboxPolicy.env_allowlist` sont transmises (aucun secret par défaut).
* Délai maximal, plafond de sortie et annulation sont appliqués par le noyau
  (le processus est tué et relancé). L'isolation réseau/mémoire/fichiers
  relève de la couche conteneur (`SandboxPolicy::to_container_args`).
* `stdout` est réservé au protocole ; les diagnostics vont sur `stderr`.
* Le niveau d'évidence (`computed`, `simulated`, …) déclaré par le composant
  peut être **abaissé** par le moteur de validation, jamais relevé ici.

## Exemples fournis

| Script | Capacité | Dépendances | Comportement dégradé |
|---|---|---|---|
| `examples/numpy_linear_solver.py` | résolution `A x = b`, résidu, conditionnement | numpy | — |
| `examples/scipy_ode_component.py` | `dx/dt = M x` | numpy, scipy (optionnel) | RK4 pas fixe en numpy si SciPy absent ; `metrics.backend_scipy` l'indique et un avertissement est émis |

## Tests

```bash
python3 -I -m unittest discover -s adapters/python/tests
cargo test -p simorph-adapter-python-bridge   # exécute les exemples de bout en bout
```

Le protocole filaire est décrit dans `adapters/PROTOCOL.md`.
