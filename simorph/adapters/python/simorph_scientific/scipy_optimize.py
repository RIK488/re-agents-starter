#!/usr/bin/env python3
"""``scipy.optimize`` as a SIMORPH component.

Two problem kinds, selected by input ``kind``:

``nonlinear`` — minimise ``objective`` (expression in ``variables``)
  inputs: objective (str), variables (list[str]), bounds (list[[lo, hi]]),
          x0 (list, optional), method: Nelder-Mead | L-BFGS-B | SLSQP |
          trust-constr | differential_evolution (default L-BFGS-B),
          constraints (list of expressions g(x) >= 0, SLSQP/trust-constr/DE),
          maxiter, seed (DE)
  The gradient (and constraint Jacobians) are derived symbolically by SymPy.

``linear`` — ``scipy.optimize.linprog`` (HiGHS)
  inputs: c, A_ub, b_ub, A_eq, b_eq, bounds; minimises c·x.
"""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from simorph_scientific.common import compile_expr, finish, sp  # noqa: E402

import numpy as np  # noqa: E402
import scipy  # noqa: E402
from scipy import optimize  # noqa: E402

from simorph_adapter import Component, Manifest, Port, ReferenceCase, Request, Result, serve  # noqa: E402

NL_METHODS = ("Nelder-Mead", "L-BFGS-B", "SLSQP", "trust-constr", "differential_evolution", "Powell")


class ScipyOptimize(Component):
    def manifest(self) -> Manifest:
        return Manifest(
            name="scipy.optimize",
            version="1.0.0",
            provider=f"SciPy {scipy.__version__} (scipy.optimize, HiGHS for LP)",
            license="BSD-3-Clause",
            capabilities=["optimisation"],
            supported_domains=["mathematics", "operations-research", "engineering"],
            inputs=[Port("kind", "1", "`nonlinear` or `linear`")],
            outputs=[
                Port("x", "1", "optimal point", shape=[0]),
                Port("fun", "1", "objective at x"),
                Port("success", "1", "1 if the solver reports success"),
                Port("nfev", "1", "objective evaluations"),
                Port("max_constraint_violation", "1"),
            ],
            assumptions=[
                "local methods return a local optimum; differential_evolution is a stochastic global heuristic",
                "linear programs are solved exactly up to HiGHS feasibility/optimality tolerances (1e-7)",
                "expressions are parsed by a whitelist AST parser (no eval); gradients are symbolic",
            ],
            uncertainty_model={"kind": "none"},
            deterministic_mode="with_seed",
            dependencies=[f"scipy=={scipy.__version__}", f"sympy=={sp.__version__}"],
            reference_cases=[ReferenceCase(
                name="Wyndor Glass LP (Hillier & Lieberman)",
                inputs={"kind": "linear", "c": [-3, -5], "A_ub": [[1, 0], [0, 2], [3, 2]], "b_ub": [4, 12, 18],
                        "bounds": [[0, None], [0, None]]},
                expected_outputs={"x": [2.0, 6.0], "fun": -36.0},
                rtol=1e-7, atol=1e-7, source="textbook optimum x=(2,6), z=36")],
            citation="Virtanen et al., SciPy 1.0 (2020); Huangfu & Hall, HiGHS (2018)",
        )

    def execute(self, request: Request) -> Result:
        inp = dict(request.config) if isinstance(request.config, dict) else {}
        inp.update(request.inputs)
        kind = inp.get("kind", "nonlinear")
        t0 = time.perf_counter()
        if kind == "linear":
            res = self._linear(request, inp)
        elif kind == "nonlinear":
            res = self._nonlinear(request, inp)
        else:
            raise ValueError("kind must be `nonlinear` or `linear`")
        res.metric("solver_wall_s", time.perf_counter() - t0)
        return finish(res)

    def _linear(self, request, inp):
        def arr(k):
            v = inp.get(k)
            return None if v is None else np.asarray(v, dtype=float)
        bounds = [tuple(None if b is None else float(b) for b in pair) for pair in inp.get("bounds", [])] or None
        r = optimize.linprog(arr("c"), A_ub=arr("A_ub"), b_ub=arr("b_ub"), A_eq=arr("A_eq"), b_eq=arr("b_eq"),
                             bounds=bounds, method="highs")
        out = Result(request, evidence="computed")
        if not r.success:
            out.warn(f"linprog: {r.message}")
        x = r.x if r.x is not None else np.full(len(inp["c"]), np.nan)
        viol = 0.0
        if inp.get("A_ub") is not None and r.x is not None:
            viol = max(0.0, float(np.max(arr("A_ub") @ x - arr("b_ub"))))
        out.output("x", [float(v) for v in x]).output("fun", float(r.fun) if r.fun is not None else None)
        out.output("success", 1 if r.success else 0).output("nfev", int(getattr(r, "nit", 0)))
        out.output("max_constraint_violation", viol)
        return out

    def _nonlinear(self, request, inp):
        names = [str(v) for v in inp["variables"]]
        syms = [sp.Symbol(n, real=True) for n in names]
        f_expr = compile_expr(inp["objective"], names)
        f = sp.lambdify([syms], f_expr, modules="numpy")
        g = sp.lambdify([syms], [sp.diff(f_expr, s) for s in syms], modules="numpy")
        bounds = [(float(lo), float(hi)) for lo, hi in inp["bounds"]]
        x0 = np.asarray(inp.get("x0") or [0.5 * (lo + hi) for lo, hi in bounds], dtype=float)
        method = str(inp.get("method", "L-BFGS-B"))
        if method not in NL_METHODS:
            raise ValueError(f"method must be one of {NL_METHODS}")
        cons_exprs = [compile_expr(c, names) for c in inp.get("constraints") or []]
        cons = []
        for ce in cons_exprs:
            cf = sp.lambdify([syms], ce, modules="numpy")
            cj = sp.lambdify([syms], [sp.diff(ce, s) for s in syms], modules="numpy")
            cons.append({"type": "ineq", "fun": (lambda x, _f=cf: float(_f(x))), "jac": (lambda x, _j=cj: np.asarray(_j(x), dtype=float))})
        fun = lambda x: float(f(x))  # noqa: E731
        jac = lambda x: np.asarray(g(x), dtype=float)  # noqa: E731
        maxiter = int(inp.get("maxiter", 2000))
        if method == "differential_evolution":
            dc = [optimize.NonlinearConstraint(c["fun"], 0.0, np.inf) for c in cons]
            r = optimize.differential_evolution(fun, bounds, seed=int(inp.get("seed", request.seed or 0)),
                                                maxiter=maxiter, tol=1e-10, polish=True, constraints=dc)
        else:
            opts = {"maxiter": maxiter}
            kw = {}
            if method in ("L-BFGS-B", "SLSQP", "trust-constr"):
                kw["jac"] = jac
            if cons and method in ("SLSQP", "trust-constr"):
                kw["constraints"] = cons
            elif cons:
                raise ValueError(f"method {method} does not support constraints")
            r = optimize.minimize(fun, x0, method=method, bounds=bounds, options=opts, **kw)
        viol = max([0.0] + [max(0.0, -c["fun"](r.x)) for c in cons])
        out = Result(request, evidence="computed")
        out.output("x", [float(v) for v in r.x]).output("fun", float(r.fun))
        out.output("success", 1 if r.success else 0).output("nfev", int(r.nfev))
        out.output("max_constraint_violation", float(viol))
        if not r.success:
            out.warn(f"{method}: {r.message}")
        return out


if __name__ == "__main__":
    serve(ScipyOptimize())
