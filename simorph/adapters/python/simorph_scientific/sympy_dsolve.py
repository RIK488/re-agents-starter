#!/usr/bin/env python3
"""``sympy.dsolve`` for ODEs in one unknown function as a SIMORPH component.

inputs:
  lhs, rhs   expressions using y, y1 (= y'), y2 (= y''), …, y9 and t
             (the equation is lhs = rhs; parsed by the whitelist parser)
  order      highest derivative order present
  ics        list of [k, t0, value] meaning y^(k)(t0) = value
  eval_times list of t at which the solution is evaluated numerically
  params     {name: value} substituted before solving (optional)
outputs: solution (text), values (list), verified (1 if sympy.checkodesol
         confirms the solution satisfies the ODE identically), residual_text.
"""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from simorph_scientific.common import compile_expr, finish, sp  # noqa: E402

from simorph_adapter import Component, Manifest, Port, ReferenceCase, Request, Result, serve  # noqa: E402


class SympyDsolve(Component):
    def manifest(self) -> Manifest:
        return Manifest(
            name="sympy.dsolve",
            version="1.0.0",
            provider=f"SymPy {sp.__version__} (sympy.dsolve, sympy.checkodesol)",
            license="BSD-3-Clause",
            capabilities=["symbolic_math"],
            supported_domains=["mathematics", "physics", "engineering"],
            inputs=[Port("lhs", "1"), Port("rhs", "1"), Port("order", "1"), Port("ics", "1", shape=[0, 3]),
                    Port("eval_times", "s", shape=[0])],
            outputs=[Port("solution", "1", "closed form y(t) as text"), Port("values", "1", shape=[0]),
                     Port("verified", "1", "1 if checkodesol returns True")],
            assumptions=["closed-form solution exists in SymPy's solver families (linear constant-coefficient, separable, Bernoulli, …)",
                         "verification is symbolic substitution (checkodesol), not a numerical test"],
            uncertainty_model={"kind": "none"},
            dependencies=[f"sympy=={sp.__version__}"],
            reference_cases=[ReferenceCase(
                name="damped oscillator y''+2y'+5y=0, y(0)=1, y'(0)=0",
                inputs={"lhs": "y2 + 2*y1 + 5*y", "rhs": "0", "order": 2, "ics": [[0, 0, 1], [1, 0, 0]], "eval_times": [2.0]},
                expected_outputs={"values": [float(sp.N(sp.exp(-2) * (sp.cos(4) + sp.sin(4) / 2)))], "verified": 1},
                rtol=1e-12, atol=1e-14, source="analytic: e^{-t}(cos 2t + sin(2t)/2)")],
        )

    def execute(self, request: Request) -> Result:
        inp = request.inputs
        order = int(inp["order"])
        if not 1 <= order <= 9:
            raise ValueError("order must be in 1..9")
        params = {str(k): float(v) for k, v in (inp.get("params") or {}).items()}
        t = sp.Symbol("t", real=True)
        yf = sp.Function("y")
        names = ["t", "y"] + [f"y{k}" for k in range(1, order + 1)] + list(params)
        lhs = compile_expr(inp["lhs"], names)
        rhs = compile_expr(inp["rhs"], names)
        subs = {sp.Symbol("y", real=True): yf(t)}
        for k in range(1, order + 1):
            subs[sp.Symbol(f"y{k}", real=True)] = yf(t).diff(t, k)
        for n, v in params.items():
            subs[sp.Symbol(n, real=True)] = sp.nsimplify(v)
        subs[sp.Symbol("t", real=True)] = t
        ode = sp.Eq(lhs.subs(subs), rhs.subs(subs))
        ics = {}
        for k, t0, val in inp.get("ics") or []:
            k = int(k)
            key = yf(t).diff(t, k).subs(t, sp.nsimplify(t0)) if k else yf(sp.nsimplify(t0))
            ics[key] = sp.nsimplify(val)
        w0 = time.perf_counter()
        sol = sp.dsolve(ode, yf(t), ics=ics or None)
        if isinstance(sol, list):
            sol = sol[0]
        ok, residual = sp.checkodesol(ode, sol)
        expr = sp.simplify(sol.rhs)
        values = [float(sp.N(expr.subs(t, sp.nsimplify(tt)), 17)) for tt in inp.get("eval_times") or []]
        res = Result(request, evidence="computed")
        res.output("solution", sp.sstr(expr)).output("values", values).output("verified", 1 if ok else 0)
        res.output("residual_text", sp.sstr(residual))
        res.metric("solve_wall_s", time.perf_counter() - w0)
        if not ok:
            res.warn("checkodesol could not confirm the solution")
        return finish(res)


if __name__ == "__main__":
    serve(SympyDsolve())
