#!/usr/bin/env python3
"""MuJoCo (DeepMind) rigid-body simulation as a SIMORPH component.

inputs:
  model       builtin name: pendulum | free_fall | cartpole  (or)
  mjcf        MJCF XML text (no <include>, no file references, no plugins)
  duration    simulated seconds
optional:
  timestep, integrator (Euler | RK4 | implicit | implicitfast)
  length      pendulum length [m] (builtin pendulum), mass [kg]
  qpos0, qvel0  initial generalized coordinates/velocities
  n_samples   number of samples returned (default 200)
outputs: times, qpos (samples), qpos_end, qvel_end, period (first full
         oscillation of qpos[0], interpolated zero crossings; null if not
         oscillating), energy_drift (relative change of total mechanical
         energy, null when energy is not tracked).
"""
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from simorph_scientific.common import finish  # noqa: E402

import mujoco  # noqa: E402
import numpy as np  # noqa: E402

from simorph_adapter import Component, Manifest, Port, ReferenceCase, Request, Result, serve  # noqa: E402

MAX_XML = 200_000
_FORBIDDEN = re.compile(r"<\s*include|file\s*=|<\s*plugin|<\s*extension|meshdir|texturedir|assetdir", re.I)


def builtin(name: str, inp) -> str:
    ts = float(inp.get("timestep", 0.0005))
    integ = str(inp.get("integrator", "RK4"))
    head = f'<option timestep="{ts}" integrator="{integ}" gravity="0 0 -9.81"><flag energy="enable" contact="disable"/></option>'
    if name == "pendulum":
        L = float(inp.get("length", 1.0))
        m = float(inp.get("mass", 1.0))
        return f"""<mujoco model="pendulum">{head}
  <worldbody><body name="pole" pos="0 0 0">
    <joint name="hinge" type="hinge" axis="0 1 0" damping="0"/>
    <geom type="sphere" size="0.001" pos="0 0 {-L}" mass="{m}"/>
  </body></worldbody></mujoco>"""
    if name == "free_fall":
        return f"""<mujoco model="free_fall">{head}
  <worldbody><body name="ball" pos="0 0 10">
    <freejoint/><geom type="sphere" size="0.05" mass="1"/>
  </body></worldbody></mujoco>"""
    if name == "cartpole":
        return f"""<mujoco model="cartpole">{head}
  <worldbody>
    <body name="cart" pos="0 0 0">
      <joint name="slider" type="slide" axis="1 0 0" damping="0"/>
      <geom type="box" size="0.2 0.1 0.05" mass="1"/>
      <body name="pole" pos="0 0 0">
        <joint name="hinge" type="hinge" axis="0 1 0" damping="0"/>
        <geom type="capsule" fromto="0 0 0 0 0 1" size="0.02" mass="0.1"/>
      </body>
    </body>
  </worldbody></mujoco>"""
    raise ValueError(f"unknown builtin model `{name}` (pendulum, free_fall, cartpole)")


def first_period(t, x):
    """Time between the first two downward zero crossings of x (linear interpolation)."""
    cr = []
    for i in range(1, len(x)):
        if x[i - 1] > 0 >= x[i]:
            cr.append(t[i - 1] + (t[i] - t[i - 1]) * x[i - 1] / (x[i - 1] - x[i]))
            if len(cr) == 2:
                return cr[1] - cr[0]
    return None


class MujocoSim(Component):
    def manifest(self) -> Manifest:
        return Manifest(
            name="mujoco.simulation",
            version="1.0.0",
            provider=f"MuJoCo {mujoco.__version__} (Google DeepMind)",
            license="Apache-2.0",
            capabilities=["physics_engine", "ode_integration"],
            supported_domains=["robotics", "mechanics", "biomechanics", "control"],
            inputs=[Port("duration", "s", range=[0.0, 3600.0])],
            outputs=[Port("times", "s", shape=[0]), Port("qpos", "1", "samples of generalized positions", shape=[0, 0]),
                     Port("qpos_end", "1", shape=[0]), Port("qvel_end", "1", shape=[0]),
                     Port("period", "s", "first oscillation period of qpos[0]"),
                     Port("energy_drift", "1", "relative change of total mechanical energy")],
            assumptions=["rigid bodies, generalized coordinates, MuJoCo soft-contact model (contacts disabled in builtin models)",
                         "builtin models use RK4 with dt = 0.5 ms unless overridden",
                         "user MJCF may not include files, assets directories or plugins"],
            uncertainty_model={"kind": "described", "description": "fixed-step integration error O(dt^4) for RK4; contact model is phenomenological"},
            dependencies=[f"mujoco=={mujoco.__version__}", f"numpy=={np.__version__}"],
            reference_cases=[ReferenceCase(
                name="free fall 1 s from 10 m", inputs={"model": "free_fall", "duration": 1.0},
                expected_outputs={"qpos_end": [0.0, 0.0, 10.0 - 0.5 * 9.81]}, rtol=0.0, atol=1e-6,
                source="analytic z = z0 - g t^2 / 2")],
            citation="Todorov, Erez, Tassa, MuJoCo, IROS 2012",
        )

    def execute(self, request: Request) -> Result:
        inp = request.inputs
        if "mjcf" in inp:
            xml = str(inp["mjcf"])
            if len(xml) > MAX_XML or _FORBIDDEN.search(xml):
                raise ValueError("MJCF rejected: too large or references files/plugins")
        else:
            xml = builtin(str(inp.get("model", "pendulum")), inp)
        model = mujoco.MjModel.from_xml_string(xml)
        data = mujoco.MjData(model)
        if "qpos0" in inp:
            data.qpos[:] = np.asarray(inp["qpos0"], dtype=float)
        if "qvel0" in inp:
            data.qvel[:] = np.asarray(inp["qvel0"], dtype=float)
        mujoco.mj_forward(model, data)
        energy_on = bool(model.opt.enableflags & mujoco.mjtEnableBit.mjENBL_ENERGY)
        e0 = float(data.energy[0] + data.energy[1]) if energy_on else None
        duration = float(inp["duration"])
        nsteps = int(round(duration / model.opt.timestep))
        ns = max(2, int(inp.get("n_samples", 200)))
        every = max(1, nsteps // (ns - 1))
        times, qs = [float(data.time)], [data.qpos.copy()]
        full_t, full_q0 = [float(data.time)], [float(data.qpos[0])] if model.nq else []
        w0 = time.perf_counter()
        for k in range(1, nsteps + 1):
            mujoco.mj_step(model, data)
            if model.nq:
                full_t.append(float(data.time))
                full_q0.append(float(data.qpos[0]))
            if k % every == 0 or k == nsteps:
                times.append(float(data.time))
                qs.append(data.qpos.copy())
        wall = time.perf_counter() - w0
        mujoco.mj_forward(model, data)
        drift = None
        if energy_on:
            e1 = float(data.energy[0] + data.energy[1])
            drift = (e1 - e0) / max(abs(e0), 1e-12)
        res = Result(request, evidence="simulated")
        res.output("times", times).output("qpos", [[float(v) for v in q] for q in qs])
        res.output("qpos_end", [float(v) for v in data.qpos]).output("qvel_end", [float(v) for v in data.qvel])
        res.output("period", first_period(full_t, full_q0) if model.nq else None)
        res.output("energy_drift", drift)
        res.metric("steps", float(nsteps)).metric("timestep", float(model.opt.timestep))
        res.metric("integrator", float(model.opt.integrator))
        res.metric("steps_per_second", nsteps / wall if wall > 0 else 0.0)
        return finish(res)


if __name__ == "__main__":
    serve(MujocoSim())
