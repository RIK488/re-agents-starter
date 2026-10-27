#!/usr/bin/env python3
"""Gymnasium (Farama Foundation) environments as a SIMORPH component.

One request = one episode.  inputs:
  env_id   one of the whitelisted classic-control environments
  seed     reset seed (determinism)
  actions  explicit action list (or)
  policy   {"kind": "linear", "weights": [...], "bias": b}
           discrete envs: action = 1 if w·obs + b > 0 else 0 (2 actions)
           continuous envs: action = clip(w·obs + b, low, high)
  max_steps (default: the env's own limit)
outputs: initial_observation, observations, rewards, total_reward, steps,
         terminated (1/0), truncated (1/0).
"""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from simorph_scientific.common import finish  # noqa: E402

import gymnasium as gym  # noqa: E402
import numpy as np  # noqa: E402

from simorph_adapter import Component, Manifest, Port, Request, Result, serve  # noqa: E402

ALLOWED = ("CartPole-v1", "Pendulum-v1", "MountainCar-v0", "MountainCarContinuous-v0", "Acrobot-v1")


class GymEnv(Component):
    def manifest(self) -> Manifest:
        return Manifest(
            name="gymnasium.episode",
            version="1.0.0",
            provider=f"Gymnasium {gym.__version__} (Farama Foundation)",
            license="MIT",
            capabilities=[{"other": "reinforcement_learning_environment"}],
            supported_domains=["ai", "reinforcement-learning", "control"],
            inputs=[Port("env_id", "1"), Port("seed", "1")],
            outputs=[Port("initial_observation", "1", shape=[0]), Port("observations", "1", shape=[0, 0]),
                     Port("rewards", "1", shape=[0]), Port("total_reward", "1"), Port("steps", "1"),
                     Port("terminated", "1"), Port("truncated", "1")],
            assumptions=["classic-control environments only (no rendering, no external assets)",
                         "episode is deterministic given seed and actions"],
            deterministic_mode="with_seed",
            dependencies=[f"gymnasium=={gym.__version__}"],
        )

    def execute(self, request: Request) -> Result:
        inp = request.inputs
        env_id = str(inp["env_id"])
        if env_id not in ALLOWED:
            raise ValueError(f"env_id must be one of {ALLOWED}")
        env = gym.make(env_id)
        obs, _ = env.reset(seed=int(inp["seed"]))
        init = [float(v) for v in np.asarray(obs).reshape(-1)]
        actions = inp.get("actions")
        policy = inp.get("policy")
        max_steps = int(inp.get("max_steps", 10**6))
        discrete = isinstance(env.action_space, gym.spaces.Discrete)
        obs_l, rew_l = [], []
        term = trunc = False
        t0 = time.perf_counter()
        for k in range(max_steps):
            if actions is not None:
                if k >= len(actions):
                    break
                a = actions[k]
                a = int(a) if discrete else np.asarray(a, dtype=np.float32).reshape(env.action_space.shape)
            elif policy is not None:
                w = np.asarray(policy["weights"], dtype=float)
                z = float(w @ np.asarray(obs, dtype=float).reshape(-1) + float(policy.get("bias", 0.0)))
                if discrete:
                    a = 1 if z > 0 else 0
                else:
                    a = np.clip(np.asarray([z], dtype=np.float32), env.action_space.low, env.action_space.high)
            else:
                raise ValueError("give `actions` or `policy`")
            obs, r, term, trunc, _ = env.step(a)
            obs_l.append([float(v) for v in np.asarray(obs).reshape(-1)])
            rew_l.append(float(r))
            if term or trunc:
                break
        env.close()
        res = Result(request, evidence="simulated")
        res.output("initial_observation", init).output("observations", obs_l).output("rewards", rew_l)
        res.output("total_reward", float(sum(rew_l))).output("steps", len(rew_l))
        res.output("terminated", 1 if term else 0).output("truncated", 1 if trunc else 0)
        res.metric("wall_s", time.perf_counter() - t0)
        return finish(res)


if __name__ == "__main__":
    serve(GymEnv())
