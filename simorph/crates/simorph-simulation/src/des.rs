//! Deterministic discrete-event simulation kernel.
//!
//! Events are closures scheduled at a simulation time; ties are broken by
//! insertion order so that two runs with the same inputs produce identical
//! event sequences.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Event handler: receives the current time, the shared state and a
/// [`Scheduler`] to enqueue follow-up events.
pub type Handler<S> = Box<dyn FnOnce(f64, &mut S, &mut Scheduler<S>) + Send>;

struct Scheduled<S> {
    time: f64,
    seq: u64,
    handler: Handler<S>,
}

impl<S> PartialEq for Scheduled<S> {
    fn eq(&self, o: &Self) -> bool {
        self.time == o.time && self.seq == o.seq
    }
}
impl<S> Eq for Scheduled<S> {}
impl<S> PartialOrd for Scheduled<S> {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl<S> Ord for Scheduled<S> {
    // Reversed so that BinaryHeap (a max-heap) pops the earliest event first,
    // and the lowest sequence number among equal times.
    fn cmp(&self, o: &Self) -> Ordering {
        o.time
            .partial_cmp(&self.time)
            .unwrap_or(Ordering::Equal)
            .then_with(|| o.seq.cmp(&self.seq))
    }
}

/// Collects events scheduled from inside a handler.
pub struct Scheduler<S> {
    pending: Vec<(f64, Handler<S>)>,
    now: f64,
}

impl<S> Scheduler<S> {
    /// Schedule `handler` at absolute time `time` (must be >= now).
    pub fn schedule_at(
        &mut self,
        time: f64,
        handler: impl FnOnce(f64, &mut S, &mut Scheduler<S>) + Send + 'static,
    ) {
        let t = if time < self.now { self.now } else { time };
        self.pending.push((t, Box::new(handler)));
    }
    /// Schedule `handler` after a delay.
    pub fn schedule_in(
        &mut self,
        delay: f64,
        handler: impl FnOnce(f64, &mut S, &mut Scheduler<S>) + Send + 'static,
    ) {
        let now = self.now;
        self.schedule_at(now + delay.max(0.0), handler);
    }
    pub fn now(&self) -> f64 {
        self.now
    }
}

/// The kernel.
pub struct DesKernel<S> {
    time: f64,
    seq: u64,
    queue: BinaryHeap<Scheduled<S>>,
    pub state: S,
    processed: u64,
}

impl<S> DesKernel<S> {
    pub fn new(state: S, t0: f64) -> Self {
        Self {
            time: t0,
            seq: 0,
            queue: BinaryHeap::new(),
            state,
            processed: 0,
        }
    }
    pub fn time(&self) -> f64 {
        self.time
    }
    pub fn processed(&self) -> u64 {
        self.processed
    }
    pub fn pending(&self) -> usize {
        self.queue.len()
    }
    fn push(&mut self, time: f64, handler: Handler<S>) {
        self.seq += 1;
        self.queue.push(Scheduled {
            time,
            seq: self.seq,
            handler,
        });
    }
    /// Schedule from outside a handler.
    pub fn schedule_at(
        &mut self,
        time: f64,
        handler: impl FnOnce(f64, &mut S, &mut Scheduler<S>) + Send + 'static,
    ) {
        let t = time.max(self.time);
        self.push(t, Box::new(handler));
    }
    /// Process the next event. Returns `false` when the queue is empty.
    pub fn step(&mut self) -> bool {
        let Some(ev) = self.queue.pop() else {
            return false;
        };
        self.time = ev.time;
        let mut sched = Scheduler {
            pending: Vec::new(),
            now: self.time,
        };
        (ev.handler)(self.time, &mut self.state, &mut sched);
        self.processed += 1;
        for (t, h) in sched.pending {
            self.push(t, h);
        }
        true
    }
    /// Process events until the next event would be after `t_end` (the
    /// clock is then advanced to `t_end`), or the queue is empty.
    pub fn run_until(&mut self, t_end: f64) -> u64 {
        let before = self.processed;
        while let Some(next) = self.queue.peek() {
            if next.time > t_end {
                break;
            }
            self.step();
        }
        if self.time < t_end {
            self.time = t_end;
        }
        self.processed - before
    }
    /// Process at most `max_events` events.
    pub fn run_all(&mut self, max_events: u64) -> u64 {
        let before = self.processed;
        while self.processed - before < max_events && self.step() {}
        self.processed - before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Queue {
        server_free_at: f64,
        departures: Vec<f64>,
        waits: Vec<f64>,
    }

    fn arrival(service: f64) -> impl FnOnce(f64, &mut Queue, &mut Scheduler<Queue>) + Send + Clone {
        move |t, q, s| {
            let start = t.max(q.server_free_at);
            q.waits.push(start - t);
            q.server_free_at = start + service;
            s.schedule_at(start + service, |t, q, _| q.departures.push(t));
        }
    }

    #[test]
    fn deterministic_single_server_queue_matches_analytic() {
        // Arrivals every 1.0, service 1.2: departure_k = 1.2 (k+1), wait_k = 0.2 k.
        let mut k = DesKernel::new(Queue::default(), 0.0);
        for i in 0..10 {
            k.schedule_at(i as f64, arrival(1.2));
        }
        k.run_all(1000);
        assert_eq!(k.state.departures.len(), 10);
        for (i, d) in k.state.departures.iter().enumerate() {
            assert!((d - 1.2 * (i as f64 + 1.0)).abs() < 1e-12);
            assert!((k.state.waits[i] - 0.2 * i as f64).abs() < 1e-12);
        }
        // Service 0.8: nobody waits.
        let mut k = DesKernel::new(Queue::default(), 0.0);
        for i in 0..10 {
            k.schedule_at(i as f64, arrival(0.8));
        }
        k.run_until(5.0);
        assert!(k.state.waits.iter().all(|w| *w == 0.0));
        assert_eq!(k.time(), 5.0);
        assert!(k.pending() > 0);
    }

    #[test]
    fn simultaneous_events_fire_in_insertion_order() {
        let mut k = DesKernel::new(Vec::<u32>::new(), 0.0);
        for i in 0..5u32 {
            k.schedule_at(1.0, move |_, v, _| v.push(i));
        }
        k.run_all(100);
        assert_eq!(k.state, vec![0, 1, 2, 3, 4]);
    }
}
