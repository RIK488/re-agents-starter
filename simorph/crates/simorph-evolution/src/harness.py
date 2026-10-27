"""SIMORPH program-evolution harness (embedded in simorph-evolution).

Executes *candidate* Python source under layered restrictions and returns the
raw solutions it produces.  Scoring is NOT done here: the Rust evaluator
recomputes feasibility and quality from the raw solutions, so a candidate
cannot influence its own score except through its output.

Layers enforced here (in addition to the process sandbox applied by the
caller: separate network namespace, scrubbed environment, timeout, output
cap, working directory):
  * static AST check: no import, no global/nonlocal, no class definitions, no
    names or attributes starting with "_", no dangerous builtins;
  * execution with a minimal builtins dictionary and the `math` module only.

Speaks the SIMORPH JSON-lines protocol (adapters/PROTOCOL.md).
argv: <manifest.json> <entry point name>
"""

import ast
import builtins
import copy
import json
import math
import sys

sys.setrecursionlimit(300)

MANIFEST = json.load(open(sys.argv[1]))
ENTRY = sys.argv[2]

SAFE_NAMES = [
    "abs", "all", "any", "bool", "dict", "enumerate", "float", "int", "len", "list", "max",
    "min", "range", "reversed", "round", "set", "sorted", "sum", "tuple", "zip", "map", "filter",
    "ValueError", "IndexError", "KeyError", "ZeroDivisionError", "Exception", "isinstance",
]
SAFE_BUILTINS = {n: getattr(builtins, n) for n in SAFE_NAMES}
FORBIDDEN = {
    "open", "exec", "eval", "compile", "input", "globals", "locals", "vars", "getattr",
    "setattr", "delattr", "breakpoint", "help", "memoryview", "type", "object", "super",
    "classmethod", "staticmethod", "property", "dir", "id", "hash", "print", "exit", "quit",
}


class Rejected(Exception):
    pass


def check(source):
    tree = ast.parse(source)
    for node in ast.walk(tree):
        if isinstance(node, (ast.Import, ast.ImportFrom, ast.Global, ast.Nonlocal, ast.ClassDef,
                             ast.AsyncFunctionDef, ast.Await, ast.AsyncFor, ast.AsyncWith)):
            raise Rejected("forbidden statement: %s" % type(node).__name__)
        if isinstance(node, ast.Attribute) and node.attr.startswith("_"):
            raise Rejected("forbidden attribute: %s" % node.attr)
        if isinstance(node, ast.Name) and (node.id.startswith("_") or node.id in FORBIDDEN):
            raise Rejected("forbidden name: %s" % node.id)
    return tree


def run(source, instances):
    try:
        tree = check(source)
        ns = {"__builtins__": SAFE_BUILTINS, "math": math}
        exec(compile(tree, "<candidate>", "exec"), ns)
        fn = ns.get(ENTRY)
        if not callable(fn):
            raise Rejected("entry point `%s` not defined" % ENTRY)
    except Exception as e:  # syntax error, rejection, definition-time error
        msg = "%s: %s" % (type(e).__name__, e)
        return [None] * len(instances), [msg] * len(instances), msg
    sols, errs = [], []
    for inst in instances:
        try:
            sols.append(fn(**copy.deepcopy(inst)))
            errs.append(None)
        except Exception as e:
            sols.append(None)
            errs.append("%s: %s" % (type(e).__name__, e))
    return sols, errs, None


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    t = msg.get("type")
    if t == "manifest":
        send({"type": "manifest", "manifest": MANIFEST})
    elif t == "execute":
        req = msg["request"]
        src = req["inputs"].get("source", "")
        insts = req["inputs"].get("instances", [])
        sols, errs, fatal = run(src, insts)
        try:
            json.dumps(sols)
        except Exception as e:
            sols, errs, fatal = [None] * len(insts), ["unserialisable output: %s" % e] * len(insts), "unserialisable output"
        send({"type": "result", "result": {
            "run_id": req["run_id"], "component": req["component"],
            "outputs": {"solutions": sols, "errors": errs, "fatal": fatal},
            "metrics": {}, "evidence": "computed", "request_hash": "", "warnings": [], "wall_time_ms": 0}})
    elif t in ("cancel", "shutdown"):
        send({"type": "ack"})
        if t == "shutdown":
            break
    else:
        send({"type": "error", "message": "unknown request type %r" % t})
