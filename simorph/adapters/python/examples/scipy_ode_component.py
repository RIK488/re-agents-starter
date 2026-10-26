#!/usr/bin/env python3
"""Linear ODE system dx/dt = M x integrated with scipy.integrate.solve_ivp
when SciPy is available, else with a pure-numpy fixed-step RK4 fallback.
The `metrics.backend_scipy` entry says which backend produced the result.
"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

import numpy as np  # noqa: E402

from simorph_adapter import Component, Manifest, Port, Request, Result, progress, serve  # noqa: E402

try:  # pragma: no cover - depends on environment
    from scipy.integrate import solve_ivp  # type: ignore

    HAVE_SCIPY = True
except Exception:  # noqa: BLE001
    HAVE_SCIPY = False


def _rk4(M: np.ndarray, x0: np.ndarray, t_end: float, steps: int):
    h = t_end / steps
    x = x0.copy()
    for _ in range(steps):
        k1 = M @ x
        k2 = M @ (x + 0.5 * h * k1)
        k3 = M @ (x + 0.5 * h * k2)
        k4 = M @ (x + h * k3)
        x = x + (h / 6.0) * (k1 + 2 * k2 + 2 * k3 + k4)
    return x


class LinearOde(Component):
    def manifest(self) -> Manifest:
        return Manifest(
            name="python.scipy.linear_ode",
            version="1.0.0",
            provider="simorph-python-adapter",
            license="BSD-3-Clause",
            capabilities=["ode_integration"],
            supported_domains=["mathematics", "dynamics"],
            inputs=[Port("matrix", "1/s", "Row-major system matrix M flattened", shape=[0]),
                    Port("x0", "1", "Initial state", shape=[0]),
                    Port("t_end", "s", "Integration horizon", range=[0.0, 1e9])],
            outputs=[Port("x_end", "1", "State at t_end", shape=[0])],
            assumptions=["linear time-invariant system", "M is constant"],
            uncertainty_model={"kind": "relative_error_bound", "rtol": 1e-8},
            deterministic_mode="always",
        )

    def execute(self, request: Request) -> Result:
        m = np.asarray(request.vec("matrix"), dtype=float)
        x0 = np.asarray(request.vec("x0"), dtype=float)
        n = x0.size
        M = m.reshape(n, n)
        t_end = request.f("t_end")
        progress(0.1, "integrating")
        res = Result(request, evidence="simulated")
        if HAVE_SCIPY:
            sol = solve_ivp(lambda t, x: M @ x, (0.0, t_end), x0, rtol=1e-10, atol=1e-12, method="DOP853")
            x_end = sol.y[:, -1]
            res.metric("backend_scipy", 1.0)
            res.metric("rhs_evaluations", float(sol.nfev))
        else:
            steps = max(1000, int(100 * t_end * max(1.0, float(np.max(np.abs(M))))))
            x_end = _rk4(M, x0, t_end, steps)
            res.metric("backend_scipy", 0.0)
            res.metric("rk4_steps", float(steps))
            res.warn("scipy unavailable: fixed-step RK4 fallback used")
        res.output("x_end", x_end.tolist())
        progress(1.0)
        return res


if __name__ == "__main__":
    serve(LinearOde())
