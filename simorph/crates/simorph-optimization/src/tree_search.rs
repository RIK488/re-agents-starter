//! Monte Carlo tree search and beam search over discrete decision problems.

use crate::problem::Method;
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use simorph_core::{ExecutionControl, Result};
use simorph_uncertainty::seeded;

/// A sequential discrete decision problem (maximise total reward).
pub trait DiscreteProblem: Sync {
    type State: Clone + Send + Sync;
    type Action: Clone + Send + Sync + PartialEq;
    fn initial_state(&self) -> Self::State;
    fn actions(&self, s: &Self::State) -> Vec<Self::Action>;
    /// Apply action, returning the next state and the immediate reward.
    fn step(&self, s: &Self::State, a: &Self::Action) -> (Self::State, f64);
    fn is_terminal(&self, s: &Self::State) -> bool;
    /// Heuristic estimate of remaining reward (used by beam search; default 0).
    fn heuristic(&self, _s: &Self::State) -> f64 {
        0.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscreteResult<A> {
    pub method: Method,
    pub actions: Vec<A>,
    pub total_reward: f64,
    pub simulations: usize,
}

struct Node<S, A> {
    state: S,
    action_from_parent: Option<A>,
    parent: Option<usize>,
    children: Vec<usize>,
    untried: Vec<A>,
    visits: f64,
    total: f64,
    terminal: bool,
}

/// UCT Monte Carlo tree search.  Runs `iterations` simulations from the
/// root then follows the most-visited children.
pub fn mcts<P: DiscreteProblem>(
    p: &P,
    iterations: usize,
    exploration: f64,
    seed: u64,
    ctl: &ExecutionControl,
) -> Result<DiscreteResult<P::Action>> {
    let mut rng = seeded(seed);
    let root_state = p.initial_state();
    let mut nodes: Vec<Node<P::State, P::Action>> = vec![Node {
        untried: p.actions(&root_state),
        terminal: p.is_terminal(&root_state),
        state: root_state,
        action_from_parent: None,
        parent: None,
        children: vec![],
        visits: 0.0,
        total: 0.0,
    }];
    let mut best_rollout: Option<(Vec<P::Action>, f64)> = None;
    for _ in 0..iterations {
        ctl.check()?;
        // selection
        let mut node = 0usize;
        let mut path_actions: Vec<P::Action> = vec![];
        let mut reward = 0.0;
        loop {
            if nodes[node].terminal {
                break;
            }
            if !nodes[node].untried.is_empty() {
                // expansion
                let k = rng.gen_range(0..nodes[node].untried.len());
                let a = nodes[node].untried.swap_remove(k);
                let (s2, r) = p.step(&nodes[node].state, &a);
                reward += r;
                let term = p.is_terminal(&s2);
                let untried = if term { vec![] } else { p.actions(&s2) };
                nodes.push(Node {
                    state: s2,
                    action_from_parent: Some(a.clone()),
                    parent: Some(node),
                    children: vec![],
                    untried,
                    visits: 0.0,
                    total: 0.0,
                    terminal: term,
                });
                let id = nodes.len() - 1;
                nodes[node].children.push(id);
                path_actions.push(a);
                node = id;
                break;
            }
            if nodes[node].children.is_empty() {
                nodes[node].terminal = true;
                break;
            }
            let ln_n = nodes[node].visits.max(1.0).ln();
            let best = *nodes[node]
                .children
                .iter()
                .max_by(|&&a, &&b| {
                    let ua = nodes[a].total / nodes[a].visits.max(1e-9)
                        + exploration * (ln_n / nodes[a].visits.max(1e-9)).sqrt();
                    let ub = nodes[b].total / nodes[b].visits.max(1e-9)
                        + exploration * (ln_n / nodes[b].visits.max(1e-9)).sqrt();
                    ua.partial_cmp(&ub).unwrap()
                })
                .unwrap();
            let (_, r) = p.step(
                &nodes[node].state,
                nodes[best].action_from_parent.as_ref().unwrap(),
            );
            reward += r;
            path_actions.push(nodes[best].action_from_parent.clone().unwrap());
            node = best;
        }
        // rollout
        let mut s = nodes[node].state.clone();
        let mut rollout_actions = vec![];
        let mut guard = 0;
        while !p.is_terminal(&s) && guard < 10_000 {
            guard += 1;
            let acts = p.actions(&s);
            if acts.is_empty() {
                break;
            }
            let a = acts[rng.gen_range(0..acts.len())].clone();
            let (s2, r) = p.step(&s, &a);
            reward += r;
            rollout_actions.push(a);
            s = s2;
        }
        let mut full = path_actions.clone();
        full.extend(rollout_actions);
        if best_rollout.as_ref().map_or(true, |(_, r)| reward > *r) {
            best_rollout = Some((full, reward));
        }
        // backpropagation
        let mut n = Some(node);
        while let Some(i) = n {
            nodes[i].visits += 1.0;
            nodes[i].total += reward;
            n = nodes[i].parent;
        }
    }
    // Greedy path by visits, then evaluate it; keep best of (greedy, best rollout).
    let mut node = 0usize;
    let mut actions = vec![];
    let mut s = p.initial_state();
    let mut total = 0.0;
    while !nodes[node].children.is_empty() {
        let best = *nodes[node]
            .children
            .iter()
            .max_by(|&&a, &&b| nodes[a].visits.partial_cmp(&nodes[b].visits).unwrap())
            .unwrap();
        let a = nodes[best].action_from_parent.clone().unwrap();
        let (s2, r) = p.step(&s, &a);
        total += r;
        s = s2;
        actions.push(a);
        node = best;
    }
    // complete greedily if not terminal
    let mut guard = 0;
    while !p.is_terminal(&s) && guard < 10_000 {
        guard += 1;
        let acts = p.actions(&s);
        if acts.is_empty() {
            break;
        }
        let a = acts
            .iter()
            .max_by(|a, b| {
                let (sa, ra) = p.step(&s, a);
                let (sb, rb) = p.step(&s, b);
                (ra + p.heuristic(&sa))
                    .partial_cmp(&(rb + p.heuristic(&sb)))
                    .unwrap()
            })
            .unwrap()
            .clone();
        let (s2, r) = p.step(&s, &a);
        total += r;
        s = s2;
        actions.push(a);
    }
    let (actions, total) = match best_rollout {
        Some((acts, r)) if r > total => (acts, r),
        _ => (actions, total),
    };
    Ok(DiscreteResult {
        method: Method::MonteCarloTreeSearch,
        actions,
        total_reward: total,
        simulations: iterations,
    })
}

/// Beam search keeping the `width` best partial solutions by
/// accumulated reward + heuristic.
pub fn beam_search<P: DiscreteProblem>(
    p: &P,
    width: usize,
    max_depth: usize,
    ctl: &ExecutionControl,
) -> Result<DiscreteResult<P::Action>> {
    let mut beam: Vec<(P::State, Vec<P::Action>, f64)> = vec![(p.initial_state(), vec![], 0.0)];
    let mut best: Option<(Vec<P::Action>, f64)> = None;
    for _ in 0..max_depth {
        ctl.check()?;
        let mut next = vec![];
        for (s, acts, r) in &beam {
            if p.is_terminal(s) {
                if best.as_ref().map_or(true, |(_, br)| r > br) {
                    best = Some((acts.clone(), *r));
                }
                continue;
            }
            for a in p.actions(s) {
                let (s2, dr) = p.step(s, &a);
                let mut na = acts.clone();
                na.push(a);
                next.push((s2, na, r + dr));
            }
        }
        if next.is_empty() {
            break;
        }
        next.sort_by(|a, b| {
            (b.2 + p.heuristic(&b.0))
                .partial_cmp(&(a.2 + p.heuristic(&a.0)))
                .unwrap()
        });
        next.truncate(width.max(1));
        beam = next;
    }
    for (s, acts, r) in beam {
        if p.is_terminal(&s) && best.as_ref().map_or(true, |(_, br)| r > *br) {
            best = Some((acts, r));
        }
    }
    let (actions, total_reward) = best.unwrap_or((vec![], f64::NEG_INFINITY));
    Ok(DiscreteResult {
        method: Method::BeamSearch,
        actions,
        total_reward,
        simulations: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0/1 knapsack: items (value, weight), capacity.  State = (index, used weight).
    struct Knapsack {
        items: Vec<(f64, f64)>,
        capacity: f64,
    }
    impl DiscreteProblem for Knapsack {
        type State = (usize, f64);
        type Action = bool;
        fn initial_state(&self) -> Self::State {
            (0, 0.0)
        }
        fn actions(&self, s: &Self::State) -> Vec<bool> {
            if s.1 + self.items[s.0].1 <= self.capacity {
                vec![true, false]
            } else {
                vec![false]
            }
        }
        fn step(&self, s: &Self::State, a: &bool) -> (Self::State, f64) {
            if *a {
                ((s.0 + 1, s.1 + self.items[s.0].1), self.items[s.0].0)
            } else {
                ((s.0 + 1, s.1), 0.0)
            }
        }
        fn is_terminal(&self, s: &Self::State) -> bool {
            s.0 >= self.items.len()
        }
        fn heuristic(&self, s: &Self::State) -> f64 {
            // fractional knapsack bound on remaining items
            let mut rem: Vec<(f64, f64)> = self.items[s.0..].to_vec();
            rem.sort_by(|a, b| (b.0 / b.1).partial_cmp(&(a.0 / a.1)).unwrap());
            let mut cap = self.capacity - s.1;
            let mut v = 0.0;
            for (val, w) in rem {
                if w <= cap {
                    v += val;
                    cap -= w;
                } else {
                    v += val * cap / w;
                    break;
                }
            }
            v
        }
    }

    fn instance() -> Knapsack {
        // optimum: items {1,2,4,5} value 60+100+... let's use classic: values 60,100,120 weights 10,20,30 cap 50 -> 220
        Knapsack {
            items: vec![
                (60.0, 10.0),
                (100.0, 20.0),
                (120.0, 30.0),
                (30.0, 15.0),
                (90.0, 25.0),
                (50.0, 5.0),
            ],
            capacity: 50.0,
        }
    }

    fn brute_force(k: &Knapsack) -> f64 {
        let n = k.items.len();
        let mut best = 0.0f64;
        for mask in 0..(1u32 << n) {
            let (mut v, mut w) = (0.0, 0.0);
            for i in 0..n {
                if mask & (1 << i) != 0 {
                    v += k.items[i].0;
                    w += k.items[i].1;
                }
            }
            if w <= k.capacity {
                best = best.max(v);
            }
        }
        best
    }

    #[test]
    fn mcts_solves_knapsack_optimally() {
        let k = instance();
        let opt = brute_force(&k);
        let r = mcts(&k, 3000, 50.0, 1, &ExecutionControl::default()).unwrap();
        assert_eq!(
            r.total_reward, opt,
            "mcts {} vs opt {}",
            r.total_reward, opt
        );
        assert_eq!(r.actions.len(), k.items.len());
    }

    #[test]
    fn beam_search_solves_knapsack_with_bound_heuristic() {
        let k = instance();
        let opt = brute_force(&k);
        let r = beam_search(&k, 8, 10, &ExecutionControl::default()).unwrap();
        assert_eq!(r.total_reward, opt);
    }
}
