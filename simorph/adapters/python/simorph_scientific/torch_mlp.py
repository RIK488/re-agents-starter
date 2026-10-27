#!/usr/bin/env python3
"""PyTorch MLP regression trainer as a SIMORPH component (CPU, deterministic).

inputs: train_x (list of rows), train_y (list of rows or scalars),
        test_x, test_y, hidden (list of layer widths, default [32, 32]),
        epochs (default 500), lr (default 1e-2), seed (request seed or input)
outputs: train_mse, test_mse, n_parameters, weights_sha256 (hash of the
         trained state_dict bytes, identical across runs with the same seed)
If ``torch`` cannot be imported the manifest still loads and every execute
returns an error naming the missing module (no fake training).
"""
import hashlib
import io
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from simorph_scientific.common import finish  # noqa: E402

from simorph_adapter import Component, Manifest, Port, Request, Result, serve  # noqa: E402

try:
    import torch  # noqa: E402
    TORCH_ERROR = None
except Exception as e:  # noqa: BLE001
    torch = None
    TORCH_ERROR = f"{type(e).__name__}: {e}"


class TorchMlp(Component):
    def manifest(self) -> Manifest:
        ver = torch.__version__ if torch else "absent"
        return Manifest(
            name="pytorch.mlp_regression",
            version="1.0.0",
            provider=f"PyTorch {ver}",
            license="BSD-3-Clause",
            capabilities=["training", "inference"],
            supported_domains=["ai", "machine-learning", "surrogate-modelling"],
            inputs=[Port("train_x", "1", shape=[0, 0]), Port("train_y", "1", shape=[0]),
                    Port("test_x", "1", shape=[0, 0]), Port("test_y", "1", shape=[0])],
            outputs=[Port("train_mse", "1"), Port("test_mse", "1"), Port("n_parameters", "1"), Port("weights_sha256", "1")],
            assumptions=["full-batch Adam on mean squared error, tanh activations, CPU, single thread, deterministic algorithms"],
            deterministic_mode="with_seed",
            dependencies=[f"torch=={ver}"],
        )

    def execute(self, request: Request) -> Result:
        if torch is None:
            raise RuntimeError(f"torch unavailable: {TORCH_ERROR}")
        inp = request.inputs
        seed = int(inp.get("seed", request.seed or 0))
        torch.manual_seed(seed)
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)

        def t(v):
            x = torch.tensor(v, dtype=torch.float64)
            return x.reshape(len(v), -1)
        xtr, ytr, xte, yte = t(inp["train_x"]), t(inp["train_y"]), t(inp["test_x"]), t(inp["test_y"])
        layers, width = [], xtr.shape[1]
        for h in inp.get("hidden", [32, 32]):
            layers += [torch.nn.Linear(width, int(h)), torch.nn.Tanh()]
            width = int(h)
        layers.append(torch.nn.Linear(width, ytr.shape[1]))
        net = torch.nn.Sequential(*layers).double()
        opt = torch.optim.Adam(net.parameters(), lr=float(inp.get("lr", 1e-2)))
        loss_fn = torch.nn.MSELoss()
        w0 = time.perf_counter()
        for _ in range(int(inp.get("epochs", 500))):
            opt.zero_grad()
            loss = loss_fn(net(xtr), ytr)
            loss.backward()
            opt.step()
        with torch.no_grad():
            tr = float(loss_fn(net(xtr), ytr))
            te = float(loss_fn(net(xte), yte))
        buf = io.BytesIO()
        for k, v in net.state_dict().items():
            buf.write(k.encode())
            buf.write(v.detach().contiguous().numpy().tobytes())
        res = Result(request, evidence="computed")
        res.output("train_mse", tr).output("test_mse", te)
        res.output("n_parameters", int(sum(p.numel() for p in net.parameters())))
        res.output("weights_sha256", hashlib.sha256(buf.getvalue()).hexdigest())
        res.metric("train_wall_s", time.perf_counter() - w0)
        return finish(res)


if __name__ == "__main__":
    serve(TorchMlp())
