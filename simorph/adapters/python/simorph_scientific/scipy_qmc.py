#!/usr/bin/env python3
"""``scipy.stats.qmc`` sampling + moment estimation as a SIMORPH component.

inputs: function (expression in ``variables``), variables, bounds,
        n (power of two recommended for Sobol), sampler: sobol | lhs | halton,
        seed, scramble (default true)
outputs: mean, variance, std_error_mean (from 8 independent scrambles when
         scramble is true), centered L2 discrepancy of the unit design.
"""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from simorph_scientific.common import compile_expr, finish, sp  # noqa: E402

import numpy as np  # noqa: E402
import scipy  # noqa: E402
from scipy.stats import qmc  # noqa: E402

from simorph_adapter import Component, Manifest, Port, Request, Result, serve  # noqa: E402


class ScipyQmc(Component):
    def manifest(self) -> Manifest:
        return Manifest(
            name="scipy.stats.qmc.moments",
            version="1.0.0",
            provider=f"SciPy {scipy.__version__} (scipy.stats.qmc)",
            license="BSD-3-Clause",
            capabilities=["uncertainty_quantification"],
            supported_domains=["mathematics", "uncertainty"],
            inputs=[Port("function", "1"), Port("variables", "1", shape=[0]), Port("bounds", "1", shape=[0, 2]), Port("n", "1")],
            outputs=[Port("mean", "1"), Port("variance", "1"), Port("std_error_mean", "1"), Port("discrepancy", "1")],
            assumptions=["inputs independent and uniformly distributed on the given bounds",
                         "std_error_mean from independent randomised (scrambled) replications"],
            uncertainty_model={"kind": "per_output_std_dev"},
            deterministic_mode="with_seed",
            dependencies=[f"scipy=={scipy.__version__}"],
        )

    def execute(self, request: Request) -> Result:
        inp = request.inputs
        names = [str(v) for v in inp["variables"]]
        syms = [sp.Symbol(n, real=True) for n in names]
        f = sp.lambdify([syms], compile_expr(inp["function"], names), modules="numpy")
        lo = np.asarray([b[0] for b in inp["bounds"]], dtype=float)
        hi = np.asarray([b[1] for b in inp["bounds"]], dtype=float)
        n = int(inp["n"])
        d = len(names)
        kind = str(inp.get("sampler", "sobol"))
        seed = int(inp.get("seed", request.seed or 0))
        scramble = bool(inp.get("scramble", True))
        reps = 8 if scramble else 1
        t0 = time.perf_counter()
        means, all_vals, disc = [], [], None
        for r in range(reps):
            if kind == "sobol":
                s = qmc.Sobol(d, scramble=scramble, seed=seed + r)
            elif kind == "halton":
                s = qmc.Halton(d, scramble=scramble, seed=seed + r)
            elif kind == "lhs":
                s = qmc.LatinHypercube(d, seed=seed + r)
            else:
                raise ValueError("sampler must be sobol, halton or lhs")
            u = s.random(n)
            if disc is None:
                disc = float(qmc.discrepancy(u))
            x = qmc.scale(u, lo, hi)
            vals = np.asarray(f(x.T), dtype=float).reshape(-1)
            if vals.size == 1:
                vals = np.full(n, float(vals))
            means.append(vals.mean())
            all_vals.append(vals)
        allv = np.concatenate(all_vals)
        se = float(np.std(means, ddof=1) / np.sqrt(reps)) if reps > 1 else None
        res = Result(request, evidence="computed")
        res.output("mean", float(np.mean(means))).output("variance", float(allv.var(ddof=1)))
        res.output("std_error_mean", se).output("discrepancy", disc)
        res.metric("evaluations", float(allv.size)).metric("wall_s", time.perf_counter() - t0)
        return finish(res)


if __name__ == "__main__":
    serve(ScipyQmc())
