#!/usr/bin/env python3
"""Dense linear solver A x = b exposed as a SIMORPH component (numpy)."""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

import numpy as np  # noqa: E402

from simorph_adapter import Component, Manifest, Port, Request, Result, serve  # noqa: E402


class LinearSolver(Component):
    def manifest(self) -> Manifest:
        return Manifest(
            name="python.numpy.linear_solver",
            version="1.0.0",
            provider="simorph-python-adapter",
            license="BSD-3-Clause",
            capabilities=["linear_algebra", "algebraic_solver"],
            supported_domains=["mathematics"],
            inputs=[Port("a", "1", "Row-major square matrix flattened", shape=[0]),
                    Port("b", "1", "Right-hand side", shape=[0])],
            outputs=[Port("x", "1", "Solution", shape=[0]),
                     Port("residual", "1", "||A x - b||_2"),
                     Port("condition_number", "1")],
            assumptions=["A is square and non-singular"],
            invariants=["residual < 1e-8"],
        )

    def execute(self, request: Request) -> Result:
        a = np.asarray(request.vec("a"), dtype=float)
        b = np.asarray(request.vec("b"), dtype=float)
        n = b.size
        if a.size != n * n:
            raise ValueError(f"matrix has {a.size} entries, expected {n * n}")
        A = a.reshape(n, n)
        x = np.linalg.solve(A, b)
        res = Result(request, evidence="computed")
        res.output("x", x.tolist())
        res.output("residual", float(np.linalg.norm(A @ x - b)))
        res.output("condition_number", float(np.linalg.cond(A)))
        res.metric("n", n)
        res.metric("numpy_version", float(np.__version__.split(".")[0]))
        return res


if __name__ == "__main__":
    serve(LinearSolver())
