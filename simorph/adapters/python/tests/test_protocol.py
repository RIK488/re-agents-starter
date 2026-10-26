"""Pure-Python protocol tests: python3 -I -m unittest discover adapters/python/tests"""
import io
import json
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from simorph_adapter import Component, Manifest, Port, Request, Result, deterministic_id, serve  # noqa: E402


class Square(Component):
    def manifest(self):
        return Manifest("t.square", "1.0.0", "test", inputs=[Port("x", "1")], outputs=[Port("y", "1")])

    def execute(self, request: Request) -> Result:
        x = request.f("x")
        if x < 0:
            raise ValueError("negative")
        return Result(request).output("y", x * x).metric("calls", 1)


class ProtocolTest(unittest.TestCase):
    def run_lines(self, lines):
        out = io.StringIO()
        serve(Square(), io.StringIO("\n".join(lines) + "\n"), out)
        return [json.loads(l) for l in out.getvalue().splitlines()]

    def test_manifest_shape(self):
        (m,) = self.run_lines([json.dumps({"type": "manifest"})])
        self.assertEqual(m["type"], "manifest")
        man = m["manifest"]
        self.assertEqual(man["name"], "t.square")
        self.assertEqual(man["units"], {"x": "1", "y": "1"})
        self.assertEqual(man["id"], deterministic_id("t.square@1.0.0"))
        for key in ("input_schema", "output_schema", "uncertainty_model", "numerical_precision",
                    "deterministic_mode", "execution_interface", "validation_protocol", "provenance"):
            self.assertIn(key, man)

    def test_execute_error_malformed_shutdown(self):
        req = {"run_id": "r", "component": "c", "inputs": {"x": 3}}
        bad = {"run_id": "r2", "component": "c", "inputs": {"x": -1}}
        msgs = self.run_lines([
            json.dumps({"type": "execute", "request": req}),
            json.dumps({"type": "execute", "request": bad}),
            "garbage",
            json.dumps({"type": "shutdown"}),
            json.dumps({"type": "manifest"}),  # after shutdown: ignored
        ])
        self.assertEqual(len(msgs), 4)
        self.assertEqual(msgs[0]["type"], "result")
        self.assertEqual(msgs[0]["result"]["outputs"]["y"], 9)
        self.assertEqual(msgs[0]["result"]["evidence"], "computed")
        self.assertEqual(msgs[1]["type"], "error")
        self.assertEqual(msgs[1]["run_id"], "r2")
        self.assertEqual(msgs[2]["type"], "error")
        self.assertEqual(msgs[3]["type"], "ack")


if __name__ == "__main__":
    unittest.main()
