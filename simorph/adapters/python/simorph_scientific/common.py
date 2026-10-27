"""Shared helpers: import path, library version detection and a *safe*
expression compiler.

Security note
-------------
``sympy.parse_expr`` and ``sympify`` call ``eval`` internally and must never
receive untrusted text.  :func:`compile_expr` parses with the standard
``ast`` module, accepts only a whitelist of node types (numbers, names,
arithmetic, comparisons for events, calls to whitelisted mathematical
functions) and builds the SymPy expression itself.  Attribute access,
subscripts, lambdas, comprehensions, keywords, strings and any non-listed
function are rejected before anything is evaluated.
"""
from __future__ import annotations

import ast
import os
import sys
from typing import Dict, Iterable, List

_HERE = os.path.dirname(os.path.abspath(__file__))
_ADAPTERS = os.path.dirname(_HERE)
if _ADAPTERS not in sys.path:
    sys.path.insert(0, _ADAPTERS)

import sympy as sp  # noqa: E402

MAX_EXPR_LEN = 4000
MAX_NODES = 2000

_FUNCS = {
    "sin": sp.sin, "cos": sp.cos, "tan": sp.tan,
    "asin": sp.asin, "acos": sp.acos, "atan": sp.atan, "atan2": sp.atan2,
    "sinh": sp.sinh, "cosh": sp.cosh, "tanh": sp.tanh,
    "exp": sp.exp, "log": sp.log, "ln": sp.log, "sqrt": sp.sqrt,
    "abs": sp.Abs, "Abs": sp.Abs, "sign": sp.sign,
    "min": sp.Min, "max": sp.Max, "floor": sp.floor, "ceiling": sp.ceiling,
    "heaviside": lambda x: sp.Heaviside(x, sp.Rational(1, 2)),
}
_CONSTS = {"pi": sp.pi, "E": sp.E, "e": sp.E}


class UnsafeExpression(ValueError):
    pass


def library_version(module_name: str) -> str:
    try:
        mod = __import__(module_name)
        return str(getattr(mod, "__version__", "?"))
    except Exception:  # noqa: BLE001
        return "absent"


class _Builder:
    def __init__(self, symbols: Dict[str, sp.Symbol], functions: Dict[str, object]):
        self.symbols = symbols
        self.functions = functions
        self.count = 0

    def build(self, node):
        self.count += 1
        if self.count > MAX_NODES:
            raise UnsafeExpression("expression too large")
        if isinstance(node, ast.Expression):
            return self.build(node.body)
        if isinstance(node, ast.Constant):
            if isinstance(node.value, bool) or not isinstance(node.value, (int, float)):
                raise UnsafeExpression(f"constant {node.value!r} not allowed")
            v = node.value
            return sp.Integer(v) if isinstance(v, int) else sp.Float(v)
        if isinstance(node, ast.Name):
            if node.id in self.symbols:
                return self.symbols[node.id]
            if node.id in _CONSTS:
                return _CONSTS[node.id]
            raise UnsafeExpression(f"unknown symbol `{node.id}`")
        if isinstance(node, ast.UnaryOp):
            v = self.build(node.operand)
            if isinstance(node.op, ast.USub):
                return -v
            if isinstance(node.op, ast.UAdd):
                return v
            raise UnsafeExpression("unary operator not allowed")
        if isinstance(node, ast.BinOp):
            a, b = self.build(node.left), self.build(node.right)
            op = node.op
            if isinstance(op, ast.Add):
                return a + b
            if isinstance(op, ast.Sub):
                return a - b
            if isinstance(op, ast.Mult):
                return a * b
            if isinstance(op, ast.Div):
                return a / b
            if isinstance(op, ast.Pow):
                return a ** b
            raise UnsafeExpression("binary operator not allowed")
        if isinstance(node, ast.Call):
            if not isinstance(node.func, ast.Name) or node.keywords:
                raise UnsafeExpression("only plain calls to whitelisted functions are allowed")
            name = node.func.id
            fn = self.functions.get(name) or _FUNCS.get(name)
            if fn is None:
                raise UnsafeExpression(f"function `{name}` not allowed")
            args = [self.build(a) for a in node.args]
            return fn(*args)
        raise UnsafeExpression(f"syntax `{type(node).__name__}` not allowed")


def compile_expr(text: str, names: Iterable[str], functions: Dict[str, object] | None = None) -> sp.Expr:
    """Parse `text` into a SymPy expression over `names` (safe, no eval)."""
    if not isinstance(text, str):
        raise UnsafeExpression("expression must be a string")
    if len(text) > MAX_EXPR_LEN:
        raise UnsafeExpression("expression too long")
    text = text.replace("^", "**")
    try:
        tree = ast.parse(text, mode="eval")
    except SyntaxError as e:
        raise UnsafeExpression(f"syntax error: {e.msg}") from None
    symbols = {n: sp.Symbol(n, real=True) for n in names}
    return _Builder(symbols, functions or {}).build(tree)


def symbol_list(names: Iterable[str]) -> List[sp.Symbol]:
    return [sp.Symbol(n, real=True) for n in names]


def manifest_provider(library: str, module: str) -> str:
    return f"{library} {library_version(module)} (via simorph_scientific)"


def _clean(v):
    import math
    if isinstance(v, float):
        return v if math.isfinite(v) else None
    if isinstance(v, (list, tuple)):
        return [_clean(x) for x in v]
    if isinstance(v, dict):
        return {k: _clean(x) for k, x in v.items()}
    return v


def finish(res):
    """JSON cannot carry NaN/inf (the Rust side rejects them): non-finite
    outputs become ``null`` and non-finite metrics are dropped with a warning."""
    import math
    res.outputs = _clean(res.outputs)
    bad = [k for k, v in res.metrics.items() if not math.isfinite(v)]
    for k in bad:
        del res.metrics[k]
        res.warn(f"metric `{k}` was not finite and has been dropped")
    return res
