#!/usr/bin/env python3
"""Generate docs/COMPONENT_MAP.md from `cargo metadata` (internal dependency graph)."""
import json, subprocess, sys, collections
meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=sys.argv[1] if len(sys.argv) > 1 else "."))
pkgs = {p["name"]: p for p in meta["packages"]}
deps = {n: sorted({d["name"] for d in p["dependencies"] if d["kind"] is None and d["name"] in pkgs}) for n, p in pkgs.items()}
ext = {n: sorted({d["name"] for d in p["dependencies"] if d["kind"] is None and d["name"] not in pkgs}) for n, p in pkgs.items()}
rdeps = collections.defaultdict(set)
for n, ds in deps.items():
    for d in ds:
        rdeps[d].add(n)
def reach(roots):
    seen, stack = set(), list(roots)
    while stack:
        x = stack.pop()
        if x in seen: continue
        seen.add(x); stack.extend(deps.get(x, []))
    return seen
apps = [n for n in pkgs if n in ("simorph-cli", "simorph-web")]
on_path = reach(apps)
layer = {}
def depth(n, stack=()):
    if n in layer: return layer[n]
    layer[n] = 0 if not deps[n] else 1 + max(depth(d, stack + (n,)) for d in deps[n])
    return layer[n]
for n in pkgs: depth(n)
path = lambda n: pkgs[n]["manifest_path"].split("/simorph/")[-1].rsplit("/Cargo.toml", 1)[0]
out = ["# SIMORPH — Cartographie des composants et dépendances", "",
       "Généré par `scripts/component-map.py` à partir de `cargo metadata` (dépendances normales, hors tests).",
       "La colonne « Chemin principal » indique si le crate est atteint depuis `simorph-cli` ou `simorph-web`,",
       "c'est-à-dire s'il sert réellement aux applications (et pas seulement à ses propres tests).", "",
       "| Couche | Crate | Répertoire | Dépend de (internes) | Utilisé par | Externes | Chemin principal |", "|---|---|---|---|---|---|---|"]
for n in sorted(pkgs, key=lambda n: (layer[n], n)):
    out.append(f"| {layer[n]} | {n} | `{path(n)}` | {', '.join(deps[n]) or '—'} | {', '.join(sorted(rdeps[n])) or '—'} | {', '.join(ext[n]) or '—'} | {'oui' if n in on_path else '**non**'} |")
off = sorted(n for n in pkgs if n not in on_path and n not in apps and n != "simorph-integration-tests")
out += ["", f"Crates hors du chemin des applications : {', '.join(off) if off else 'aucun'}.", ""]
print("\n".join(out))
