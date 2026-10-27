#!/usr/bin/env bash
# Creates simorph/.venv with the scientific Python stack used by the adapters.
# PyTorch: the CPU index (download.pytorch.org) may be blocked by proxies; the
# PyPI wheel is used as a fallback.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -m venv .venv
./.venv/bin/pip install --upgrade pip
./.venv/bin/pip install numpy scipy sympy z3-solver mujoco gymnasium
./.venv/bin/pip install torch --index-url https://download.pytorch.org/whl/cpu || ./.venv/bin/pip install torch
