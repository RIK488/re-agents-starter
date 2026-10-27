#!/usr/bin/env python3
"""``scipy.integrate.solve_ivp`` as a SIMORPH component.

Inputs
  equations   list[str]  right-hand side of dy_i/dt, in terms of the state
                         names, ``t`` and the parameter names (safe parser)
  state_names list[str]
  y0          list[float]
  t_end       float      (t0 = input ``t0`` if given, else 0)
Optional inputs / config
  params      {name: value}
  method      RK45 | DOP853 | Radau | BDF | LSODA   (default RK45)
  rtol, atol  tolerances (default 1e-8, 1e-10)
  n_samples   number of dense-output samples returned (default 50)
  events      list[str] expressions; integration stops at the first zero
              crossing of a *terminal* event (all events are terminal)
  max_step    float
For implicit methods (Radau, BDF, LSODA) the Jacobian is derived
symbolically with SymPy and passed to SciPy.
"""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))  # adapters/python (-I drops it)

from simorph_scientific.common import compile_expr, finish, sp  # noqa: E402

import numpy as np
import scipy
from scipy.integrate import solve_ivp

from simorph_adapter import Component, Manifest, Port, ReferenceCase, Request, Result, serve

METHODS = ("RK45", "DOP853", "Radau", "BDF", "LSODA", "RK23")


class ScipyIvp(Component):
    def manifest(self) -> Manifest:
        return Manifest(
            name="scipy.integrate.solve_ivp",
            version="1.0.0",
            provider=f"SciPy {scipy.__version__} (scipy.integrate.solve_ivp)",
            license="BSD-3-Clause",
            capabilities=["ode_integration"],
            supported_domains=["mathematics", "physics", "chemistry", "engineering"],
            inputs=[
                Port("equations", "1", "RHS expressions dy_i/dt (one per state)", shape=[0]),
                Port("state_names", "1", "state variable names", shape=[0]),
                Port("y0", "1", "initial state", shape=[0]),
                Port("t_end", "s", "final time"),
            ],
            outputs=[
                Port("y_end", "1", "state at the final time", shape=[0]),
                Port("t_final", "s"),
                Port("times", "s", "dense-output sample times", shape=[0]),
                Port("trajectory", "1", "samples[i][k] of state k at times[i]", shape=[0, 0]),
                Port("nfev", "1", "right-hand side evaluations"),
                Port("njev", "1", "Jacobian evaluations"),
                Port("nlu", "1", "LU decompositions"),
                Port("event_time", "s", "time of the terminal event (null if none)"),
            ],
            assumptions=[
                "ODE given explicitly as dy/dt = f(t, y, p); expressions are parsed by a whitelist AST parser (no eval)",
                "implicit methods use the exact symbolic Jacobian computed by SymPy",
                "error control is local (rtol, atol); global error is not bounded by the tolerances",
            ],
            validity_conditions=["f must be finite on the integration interval"],
            uncertainty_model={"kind": "described", "description": "local error per step <= atol + rtol*|y| (SciPy embedded error estimator)"},
            dependencies=[f"scipy=={scipy.__version__}", f"numpy=={np.__version__}", f"sympy=={sp.__version__}"],
            reference_cases=[ReferenceCase(
                name="exponential decay",
                inputs={"equations": ["-k*y"], "state_names": ["y"], "y0": [1.0], "t_end": 2.0,
                        "params": {"k": 0.5}, "method": "DOP853", "rtol": 1e-10, "atol": 1e-12},
                expected_outputs={"y_end": [float(np.exp(-1.0))]},
                rtol=1e-8, atol=1e-12, source="analytic solution y = exp(-k t)")],
            invariants=["nfev > 0"],
            citation="Virtanen et al., SciPy 1.0, Nature Methods 17 (2020)",
        )

    def execute(self, request: Request) -> Result:
        cfg = request.config if isinstance(request.config, dict) else {}
        inp = dict(cfg)
        inp.update(request.inputs)
        names = [str(n) for n in inp["state_names"]]
        eqs = inp["equations"]
        if len(eqs) != len(names):
            raise ValueError(f"{len(eqs)} equations for {len(names)} states")
        params = {str(k): float(v) for k, v in (inp.get("params") or {}).items()}
        method = str(inp.get("method", "RK45"))
        if method not in METHODS:
            raise ValueError(f"method must be one of {METHODS}")
        rtol = float(inp.get("rtol", 1e-8))
        atol = float(inp.get("atol", 1e-10))
        n_samples = max(2, int(inp.get("n_samples", 50)))
        t0 = float(inp.get("t0", 0.0))
        t_end = float(inp["t_end"])
        y0 = np.asarray([float(v) for v in inp["y0"]], dtype=float)
        all_names = names + ["t"] + list(params)
        exprs = [compile_expr(e, all_names) for e in eqs]
        ys = [sp.Symbol(n, real=True) for n in names]
        tsym = sp.Symbol("t", real=True)
        psyms = [sp.Symbol(n, real=True) for n in params]
        pvals = [params[n] for n in params]
        args = [tsym, ys] + psyms
        f_num = sp.lambdify(args, exprs, modules="numpy")
        jac_expr = sp.Matrix(exprs).jacobian(sp.Matrix(ys))
        j_num = sp.lambdify(args, jac_expr, modules="numpy")

        def rhs(t, y):
            return np.asarray(f_num(t, y, *pvals), dtype=float).reshape(-1)

        def jac(t, y):
            return np.asarray(j_num(t, y, *pvals), dtype=float).reshape(len(names), len(names))

        events = []
        for e in inp.get("events") or []:
            ev = sp.lambdify(args, compile_expr(e, all_names), modules="numpy")

            def g(t, y, _ev=ev):
                return float(_ev(t, y, *pvals))
            g.terminal = True
            events.append(g)

        kwargs = dict(method=method, rtol=rtol, atol=atol, dense_output=True)
        if method in ("Radau", "BDF", "LSODA"):
            kwargs["jac"] = jac
        if "max_step" in inp:
            kwargs["max_step"] = float(inp["max_step"])
        if events:
            kwargs["events"] = events
        w0 = time.perf_counter()
        sol = solve_ivp(rhs, (t0, t_end), y0, **kwargs)
        wall = time.perf_counter() - w0
        if sol.status < 0:
            raise RuntimeError(f"solve_ivp failed: {sol.message}")
        tf = float(sol.t[-1])
        ts = np.linspace(t0, tf, n_samples)
        traj = sol.sol(ts).T
        ev_t = None
        if events and sol.t_events is not None:
            hits = [float(te[0]) for te in sol.t_events if len(te)]
            if hits:
                ev_t = min(hits)
        res = Result(request, evidence="simulated")
        res.output("y_end", [float(v) for v in sol.y[:, -1]])
        res.output("t_final", tf)
        res.output("times", [float(v) for v in ts])
        res.output("trajectory", [[float(v) for v in row] for row in traj])
        res.output("nfev", int(sol.nfev))
        res.output("njev", int(sol.njev))
        res.output("nlu", int(sol.nlu))
        res.output("event_time", ev_t)
        res.metric("steps", float(len(sol.t) - 1))
        res.metric("solver_wall_s", wall)
        res.metric("scipy_version_major", float(scipy.__version__.split(".")[0]))
        if sol.status == 1:
            res.warn("integration stopped by a terminal event")
        return finish(res)


if __name__ == "__main__":
    serve(ScipyIvp())
