"""SIMORPH formal-verification helper (subsystem 13).

Single entry point, one JSON request on stdin, one JSON response on stdout.
Run with ``python -I formal_tool.py``.  The Rust side (crate
``simorph-formal``) only sends text it generated itself from a validated
AST, never raw user input; this script additionally re-validates every
expression string against a strict token whitelist before handing it to
SymPy's parser.

Operations (field ``op``):

* ``versions``        – versions of z3, sympy, scipy (or the import error).
* ``z3``              – run an SMT-LIB 2 script with Z3; report sat/unsat/unknown
                        and the model of the declared constants.
* ``linprog``         – optimise a linear objective with scipy.optimize.linprog.
* ``sympy_ode``       – check that a closed form satisfies an explicit ODE.
* ``sympy_equiv``     – check symbolic equivalence of two expressions.
* ``sympy_jacobian``  – symbolic Jacobian of a vector field, evaluated at a point.

Resource limits: the request may carry ``memory_mb``; the address-space limit
is applied with ``resource.setrlimit`` before any solver is imported.
"""

import json
import random
import re
import sys
import time

ALLOWED_FUNCS = {
    "sin", "cos", "tan", "exp", "log", "sqrt", "Abs", "atan", "asin", "acos",
    "sinh", "cosh", "tanh", "pi", "E", "Rational", "Integer",
}
TOKEN_RE = re.compile(r"\s*(?:(\d+(?:\.\d*)?(?:[eE][-+]?\d+)?)|([A-Za-z_][A-Za-z0-9_]*)|(\*\*|[-+*/(),]))")


def _limit(req):
    mb = int(req.get("memory_mb") or 0)
    if mb > 0:
        try:
            import resource
            lim = mb * 1024 * 1024
            resource.setrlimit(resource.RLIMIT_AS, (lim, lim))
        except Exception as e:  # pragma: no cover - platform specific
            sys.stderr.write(f"rlimit not applied: {e}\n")


def _check_expr(text, symbols):
    """Reject anything that is not numbers, declared symbols, whitelisted
    functions and arithmetic operators."""
    pos = 0
    while pos < len(text):
        m = TOKEN_RE.match(text, pos)
        if not m or m.end() == pos:
            if text[pos:].strip() == "":
                break
            raise ValueError(f"forbidden character at {pos}: {text[pos:pos+10]!r}")
        ident = m.group(2)
        if ident is not None and ident not in symbols and ident not in ALLOWED_FUNCS:
            raise ValueError(f"identifier not allowed: {ident!r}")
        if "__" in (ident or ""):
            raise ValueError("dunder identifiers are not allowed")
        pos = m.end()


def _sympy_ns(symbols, positive):
    import sympy as sp
    ns = {name: getattr(sp, name) for name in ALLOWED_FUNCS if hasattr(sp, name)}
    syms = {s: sp.Symbol(s, positive=True) if positive else sp.Symbol(s, real=True) for s in symbols}
    ns.update(syms)
    return ns, syms


def _parse(text, ns, symbols):
    import sympy as sp
    from sympy.parsing.sympy_parser import parse_expr, standard_transformations
    _check_expr(text, symbols)
    return parse_expr(text, local_dict=dict(ns), global_dict={"__builtins__": {}, **{k: getattr(sp, k) for k in ("Integer", "Float", "Rational", "Symbol")}}, transformations=standard_transformations, evaluate=True)


def _spot(expr, syms, samples, rng, ranges):
    import sympy as sp
    f = sp.lambdify(list(syms.values()), expr, modules="math")
    worst = 0.0
    for _ in range(samples):
        args = []
        for name in syms:
            lo, hi = ranges.get(name, (0.5, 2.0))
            args.append(rng.uniform(lo, hi))
        try:
            v = abs(float(f(*args)))
        except (ZeroDivisionError, ValueError, OverflowError):
            continue
        worst = max(worst, v)
    return worst


def op_versions(_req):
    out = {}
    for mod in ("z3", "sympy", "scipy"):
        try:
            m = __import__(mod)
            out[mod] = m.get_version_string() if mod == "z3" else m.__version__
        except Exception as e:
            out[mod] = None
            out[mod + "_error"] = str(e)
    return out


def op_z3(req):
    import z3
    t0 = time.time()
    s = z3.Solver()
    s.set("timeout", int(req.get("timeout_ms", 10000)))
    s.from_string(req["smtlib"])
    r = s.check()
    out = {"solver": "z3", "version": z3.get_version_string(), "elapsed_ms": 0.0}
    if r == z3.sat:
        out["result"] = "sat"
        m = s.model()
        model = {}
        declared = set(req.get("declared", []))
        for d in m.decls():
            name = d.name()
            if declared and name not in declared:
                continue
            v = m[d]
            entry = {"exact": str(v)}
            try:
                if z3.is_int_value(v):
                    entry["approx"] = float(v.as_long())
                elif z3.is_rational_value(v):
                    entry["approx"] = float(v.numerator_as_long()) / float(v.denominator_as_long())
                elif z3.is_algebraic_value(v):
                    entry["approx"] = float(v.approx(20).as_fraction())
                elif z3.is_true(v) or z3.is_false(v):
                    entry["approx"] = 1.0 if z3.is_true(v) else 0.0
            except Exception as e:  # pragma: no cover
                entry["error"] = str(e)
            model[name] = entry
        out["model"] = model
    elif r == z3.unsat:
        out["result"] = "unsat"
    else:
        out["result"] = "unknown"
        out["reason"] = s.reason_unknown()
    out["elapsed_ms"] = (time.time() - t0) * 1000.0
    return out


def op_linprog(req):
    import numpy as np
    from scipy.optimize import linprog
    import scipy
    c = np.array(req["c"], dtype=float)
    sense = req.get("sense", "max")
    obj = -c if sense == "max" else c
    kw = {}
    if req.get("A_ub"):
        kw["A_ub"] = np.array(req["A_ub"], dtype=float)
        kw["b_ub"] = np.array(req["b_ub"], dtype=float)
    if req.get("A_eq"):
        kw["A_eq"] = np.array(req["A_eq"], dtype=float)
        kw["b_eq"] = np.array(req["b_eq"], dtype=float)
    bounds = [(lo, hi) for lo, hi in req["bounds"]]
    res = linprog(obj, bounds=bounds, method="highs", **kw)
    out = {"method": "scipy.optimize.linprog/highs", "version": scipy.__version__, "status": int(res.status), "message": str(res.message)}
    if res.status == 0:
        out["value"] = float(-res.fun if sense == "max" else res.fun)
        out["x"] = [float(v) for v in res.x]
    return out


def op_sympy_ode(req):
    """Explicit ODE system dx_i/dt = f_i(t, x); claimed solutions x_i(t)."""
    import sympy as sp
    symbols = list(req["symbols"])
    ns, syms = _sympy_ns(symbols, req.get("positive", False))
    t = syms[req["time"]]
    states = req["states"]
    sol = {s: _parse(req["solution"][s], ns, symbols) for s in states}
    residuals = []
    for s in states:
        rhs = _parse(req["rhs"][s], ns, symbols)
        sub = rhs.subs({syms[x]: sol[x] for x in states}, simultaneous=True)
        r = sp.simplify(sp.diff(sol[s], t) - sub)
        residuals.append(r)
    init = {}
    for s in states:
        if s + "_0" in syms:
            init[s] = str(sp.simplify(sol[s].subs(t, 0) - syms[s + "_0"]))
    rng = random.Random(int(req.get("seed", 1)))
    spot = max(_spot(r, syms, int(req.get("samples", 50)), rng, req.get("ranges", {})) for r in residuals) if residuals else 0.0
    return {
        "sympy": sp.__version__,
        "residuals": [str(r) for r in residuals],
        "is_zero": all(r == 0 for r in residuals),
        "initial_condition_residuals": init,
        "spot_max_abs": spot,
    }


def op_sympy_equiv(req):
    import sympy as sp
    symbols = list(req["symbols"])
    ns, syms = _sympy_ns(symbols, req.get("positive", True))
    a = _parse(req["a"], ns, symbols)
    b = _parse(req["b"], ns, symbols)
    diff = sp.simplify(a - b)
    ratio = None
    if diff != 0:
        try:
            q = sp.simplify(a / b)
            ratio = str(q)
        except Exception:
            ratio = None
    rng = random.Random(int(req.get("seed", 1)))
    spot = _spot(a - b, syms, int(req.get("samples", 50)), rng, req.get("ranges", {}))
    return {"sympy": sp.__version__, "difference": str(diff), "is_zero": diff == 0, "ratio": ratio, "spot_max_abs": spot}


def op_sympy_jacobian(req):
    import sympy as sp
    symbols = list(req["symbols"])
    ns, syms = _sympy_ns(symbols, False)
    states = req["states"]
    f = sp.Matrix([_parse(e, ns, symbols) for e in req["rhs"]])
    x = sp.Matrix([syms[s] for s in states])
    j = f.jacobian(x)
    point = {syms[k]: v for k, v in req["point"].items()}
    jn = j.subs(point)
    values = [[float(jn[i, k]) for k in range(len(states))] for i in range(len(req["rhs"]))]
    return {"sympy": sp.__version__, "jacobian": [[str(j[i, k]) for k in range(len(states))] for i in range(len(req["rhs"]))], "values": values}


OPS = {
    "versions": op_versions,
    "z3": op_z3,
    "linprog": op_linprog,
    "sympy_ode": op_sympy_ode,
    "sympy_equiv": op_sympy_equiv,
    "sympy_jacobian": op_sympy_jacobian,
}


def main():
    req = json.loads(sys.stdin.read())
    _limit(req)
    op = req.get("op")
    if op not in OPS:
        print(json.dumps({"error": f"unknown op {op!r}"}))
        return 2
    try:
        out = OPS[op](req)
        out["ok"] = True
    except Exception as e:
        out = {"ok": False, "error": f"{type(e).__name__}: {e}"}
    sys.stdout.write(json.dumps(out))
    sys.stdout.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main())
