"""simorph_adapter — write SIMORPH components in Python.

A component is a subclass of :class:`Component` that returns a
:class:`Manifest` and implements :meth:`Component.execute`.  :func:`serve`
runs the SIMORPH JSON-lines protocol (see ``adapters/PROTOCOL.md``) over
stdin/stdout so the Rust side (``SubprocessComponent``) can drive it.

The wire format mirrors the Rust ``simorph_core`` types field for field.
"""

from __future__ import annotations

import dataclasses
import json
import sys
import time
import uuid
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

PROTOCOL_VERSION = "1.0"

# Namespace used by simorph_core::TypedId::deterministic (UUID v5).
_SIMORPH_NS = uuid.UUID(bytes=bytes([0x5A, 0x1D, 0x0E, 0x3F, 0x9C, 0x4B, 0x4F, 0x2A,
                                     0x8E, 0x1D, 0x6B, 0x7F, 0x3C, 0x2A, 0x9D, 0x11]))


def deterministic_id(name: str) -> str:
    """Same id as ``ComponentId::deterministic(name)`` on the Rust side."""
    return str(uuid.uuid5(_SIMORPH_NS, name))


@dataclass
class Port:
    name: str
    unit: str
    description: Optional[str] = None
    shape: List[int] = field(default_factory=list)
    range: Optional[List[float]] = None

    def to_dict(self) -> Dict[str, Any]:
        d: Dict[str, Any] = {"name": self.name, "unit": self.unit, "shape": list(self.shape)}
        if self.description is not None:
            d["description"] = self.description
        if self.range is not None:
            d["range"] = [float(self.range[0]), float(self.range[1])]
        return d


@dataclass
class ReferenceCase:
    name: str
    inputs: Dict[str, Any]
    expected_outputs: Dict[str, Any]
    rtol: float
    atol: float
    source: str

    def to_dict(self) -> Dict[str, Any]:
        return dataclasses.asdict(self)


@dataclass
class Manifest:
    """Python view of ``simorph_core::ComponentManifest``."""

    name: str
    version: str
    provider: str
    license: str = "Apache-2.0"
    capabilities: List[Any] = field(default_factory=list)
    supported_domains: List[str] = field(default_factory=list)
    inputs: List[Port] = field(default_factory=list)
    outputs: List[Port] = field(default_factory=list)
    assumptions: List[str] = field(default_factory=list)
    validity_ranges: Dict[str, List[float]] = field(default_factory=dict)
    validity_conditions: List[str] = field(default_factory=list)
    uncertainty_model: Dict[str, Any] = field(default_factory=lambda: {"kind": "none"})
    numerical_precision: str = "float64"
    deterministic_mode: Any = "always"
    dependencies: List[str] = field(default_factory=list)
    reference_cases: List[ReferenceCase] = field(default_factory=list)
    invariants: List[str] = field(default_factory=list)
    security_permissions: List[str] = field(default_factory=list)
    citation: Optional[str] = None

    @property
    def id(self) -> str:
        return deterministic_id(f"{self.name}@{self.version}")

    def to_dict(self) -> Dict[str, Any]:
        units = {p.name: p.unit for p in self.inputs + self.outputs}
        return {
            "id": self.id,
            "name": self.name,
            "version": self.version,
            "provider": self.provider,
            "license": self.license,
            "capabilities": list(self.capabilities),
            "supported_domains": list(self.supported_domains),
            "input_schema": [p.to_dict() for p in self.inputs],
            "output_schema": [p.to_dict() for p in self.outputs],
            "units": units,
            "assumptions": list(self.assumptions),
            "validity_domain": {
                "ranges": {k: [float(v[0]), float(v[1])] for k, v in self.validity_ranges.items()},
                "conditions": list(self.validity_conditions),
            },
            "uncertainty_model": self.uncertainty_model,
            "numerical_precision": self.numerical_precision,
            "deterministic_mode": self.deterministic_mode,
            "resource_requirements": {"cpu_cores": 1.0, "memory_mb": 0, "gpu_count": 0,
                                      "gpu_memory_mb": 0, "wall_time_s": 0.0, "network": False, "disk_mb": 0},
            "dependencies": list(self.dependencies),
            "execution_interface": {"kind": "subprocess", "command": sys.executable or "python3",
                                    "args": ["-I", sys.argv[0] if sys.argv else ""]},
            "validation_protocol": {
                "reference_cases": [r.to_dict() for r in self.reference_cases],
                "invariants": list(self.invariants),
                "convergence_order": None,
            },
            "provenance": {"source_repository": None, "commit": None, "build_hash": None,
                           "generated_by": "simorph_adapter " + PROTOCOL_VERSION, "citation": self.citation},
            "security_permissions": list(self.security_permissions),
        }


class Request:
    """Typed view of ``simorph_core::TaskRequest``."""

    def __init__(self, raw: Dict[str, Any]):
        self.raw = raw
        self.run_id: str = raw["run_id"]
        self.component: str = raw["component"]
        self.inputs: Dict[str, Any] = raw.get("inputs", {}) or {}
        self.seed: Optional[int] = raw.get("seed")
        self.fidelity: float = float(raw.get("fidelity", 1.0))
        self.config: Any = raw.get("config")

    def f(self, name: str) -> float:
        v = self.inputs.get(name)
        if not isinstance(v, (int, float)):
            raise ValueError(f"missing numeric input `{name}`")
        return float(v)

    def vec(self, name: str) -> List[float]:
        v = self.inputs.get(name)
        if not isinstance(v, list):
            raise ValueError(f"input `{name}` is not an array")
        return [float(x) for x in v]


class Result:
    """Builder for ``simorph_core::TaskResult``."""

    def __init__(self, request: Request, evidence: str = "computed"):
        self.request = request
        self.evidence = evidence
        self.outputs: Dict[str, Any] = {}
        self.metrics: Dict[str, float] = {}
        self.warnings: List[str] = []

    def output(self, name: str, value: Any) -> "Result":
        self.outputs[name] = value
        return self

    def metric(self, name: str, value: float) -> "Result":
        self.metrics[name] = float(value)
        return self

    def warn(self, message: str) -> "Result":
        self.warnings.append(message)
        return self

    def to_dict(self, wall_time_ms: int = 0) -> Dict[str, Any]:
        return {
            "run_id": self.request.run_id,
            "component": self.request.component,
            "outputs": self.outputs,
            "metrics": self.metrics,
            "evidence": self.evidence,
            # the Rust side recomputes and overwrites the request hash
            "request_hash": "",
            "warnings": self.warnings,
            "wall_time_ms": wall_time_ms,
        }


class Component:
    """Base class for Python components."""

    def manifest(self) -> Manifest:  # pragma: no cover - abstract
        raise NotImplementedError

    def execute(self, request: Request) -> Result:  # pragma: no cover - abstract
        raise NotImplementedError

    def cancel(self) -> None:
        """Best-effort cancellation hook (called on a `cancel` message)."""


def progress(fraction: float, message: Optional[str] = None, out=None) -> None:
    """Emit a progress heartbeat."""
    out = out or sys.stdout
    msg: Dict[str, Any] = {"type": "progress", "fraction": float(fraction)}
    if message:
        msg["message"] = message
    out.write(json.dumps(msg) + "\n")
    out.flush()


def _write(out, obj: Dict[str, Any]) -> None:
    out.write(json.dumps(obj) + "\n")
    out.flush()


def serve(component: Component, inp=None, out=None) -> None:
    """Run the protocol loop until stdin closes or `shutdown` arrives."""
    inp = inp or sys.stdin
    out = out or sys.stdout
    for line in inp:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
            kind = msg["type"]
        except Exception as e:  # noqa: BLE001
            _write(out, {"type": "error", "message": f"malformed request: {e}"})
            continue
        if kind == "manifest":
            _write(out, {"type": "manifest", "manifest": component.manifest().to_dict()})
        elif kind == "execute":
            req = Request(msg["request"])
            t0 = time.perf_counter()
            try:
                res = component.execute(req)
                ms = int((time.perf_counter() - t0) * 1000)
                _write(out, {"type": "result", "result": res.to_dict(ms)})
            except Exception as e:  # noqa: BLE001
                print(f"simorph_adapter: execute failed: {e!r}", file=sys.stderr)
                _write(out, {"type": "error", "message": f"{type(e).__name__}: {e}", "run_id": req.run_id})
        elif kind == "cancel":
            component.cancel()
            _write(out, {"type": "ack"})
        elif kind == "shutdown":
            _write(out, {"type": "ack"})
            return
        else:
            _write(out, {"type": "error", "message": f"unknown request type `{kind}`"})


__all__ = ["Component", "Manifest", "Port", "ReferenceCase", "Request", "Result",
           "serve", "progress", "deterministic_id", "PROTOCOL_VERSION"]
