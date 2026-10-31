//! Paces the wheel's playhead steps against Resolve's real playhead.
//!
//! Resolve works through key presses one at a time. With footage that is slow
//! to decode (H.265 and other long-GOP codecs) a fast turn queued up dozens of
//! frame steps, so the playhead lagged and kept going after the wheel stopped.
//! The pacer keeps only a few steps in flight, reads Resolve's playhead through
//! the helper (well under a millisecond in Resolve) and drops the steps Resolve
//! could not follow, the way Resolve itself skips frames it cannot show in time.

use std::time::{Duration, Instant};

use mapper::keyboard::Shortcut;

/// Steps sent to Resolve but not yet seen on its playhead.
const MAX_IN_FLIGHT: i32 = 2;
/// Steps kept waiting while Resolve is busy; anything beyond is dropped.
const MAX_WAITING: i32 = 2;
/// In-flight steps that don't show up for this long are forgotten (start or
/// end of the timeline, another page, a dialog).
const STALL: Duration = Duration::from_millis(300);
/// A playhead query without an answer for this long counts as failed. While
/// Resolve decodes heavy footage its answers can take a few hundred
/// milliseconds; that is a reason to wait, not to stop pacing.
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
/// After a failed query, steps go straight through for this long.
const DIRECT: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Pacer {
    /// Steps not yet sent; the sign is the direction.
    waiting: i32,
    /// Steps sent, not yet seen; the sign is the direction.
    in_flight: i32,
    keys: Option<(Shortcut, Shortcut, i32)>,
    /// Last playhead reading in frames.
    position: Option<i64>,
    last_progress: Option<Instant>,
    query: Option<(u64, Instant)>,
    /// Queries given up on; their late answers are swallowed.
    expired: Vec<u64>,
    direct_until: Option<Instant>,
}

impl Pacer {
    /// Wheel steps to make (each moving the playhead `frames` frames).
    pub fn add(&mut self, steps: i32, forward: Shortcut, reverse: Shortcut, frames: i32, now: Instant) {
        let keys = Some((forward, reverse, frames));
        if self.keys != keys {
            self.keys = keys;
            self.waiting = 0;
            self.in_flight = 0;
        }
        if self.waiting.signum() == -steps.signum() {
            self.waiting = 0;
        }
        if self.in_flight.signum() == -steps.signum() {
            // Turned back: what is in flight can't be told apart any more.
            self.in_flight = 0;
        }
        if self.in_flight == 0 {
            self.last_progress = Some(now);
        }
        self.waiting = if self.direct(now) { self.waiting + steps } else { (self.waiting + steps).clamp(-MAX_WAITING, MAX_WAITING) };
    }

    /// Key presses to send now, and whether to ask Resolve for its playhead.
    pub fn pump(&mut self, now: Instant) -> (Vec<Shortcut>, bool) {
        let Some((forward, reverse, _)) = self.keys.clone() else {
            return (Vec::new(), false);
        };
        let key = |n: i32| if n > 0 { forward.clone() } else { reverse.clone() };
        if self.direct(now) {
            let taps = (0..self.waiting.abs()).map(|_| key(self.waiting)).collect();
            self.waiting = 0;
            self.in_flight = 0;
            return (taps, false);
        }
        if let Some((id, _)) = self.query.filter(|(_, at)| now.duration_since(*at) > QUERY_TIMEOUT) {
            self.query = None;
            if self.expired.len() >= 16 {
                self.expired.remove(0);
            }
            self.expired.push(id);
            self.fail(now);
            return self.pump(now);
        }
        // Only answers without movement count as a stall; a slow answer just means Resolve is busy.
        let answered = self.query.is_none();
        if self.in_flight != 0 && answered && self.last_progress.is_some_and(|t| now.duration_since(t) > STALL) {
            self.in_flight = 0;
        }
        let mut taps = Vec::new();
        // The first steps wait for a reading to compare against.
        if self.position.is_some() {
            while self.waiting != 0 && self.in_flight.abs() < MAX_IN_FLIGHT {
                let sign = self.waiting.signum();
                taps.push(key(sign));
                self.waiting -= sign;
                self.in_flight += sign;
            }
        }
        let want = (self.in_flight != 0 || self.waiting != 0) && self.query.is_none();
        (taps, want)
    }

    /// A playhead query went out with this id.
    pub fn queried(&mut self, id: u64, now: Instant) {
        self.query = Some((id, now));
    }

    /// Takes the helper's reply if it answers our query.
    pub fn on_reply(&mut self, id: Option<u64>, ok: bool, message: &str, now: Instant) -> bool {
        let Some(id) = id else { return false };
        if let Some(i) = self.expired.iter().position(|&e| e == id) {
            self.expired.remove(i);
            return true;
        }
        if self.query.map(|(q, _)| q) != Some(id) {
            return false;
        }
        self.query = None;
        let Some(frame) = ok.then(|| message.trim().parse::<i64>().ok()).flatten() else {
            self.fail(now);
            return true;
        };
        if let Some(previous) = self.position {
            let moved = (frame - previous).abs();
            if moved > 0 {
                let per_step = self.keys.as_ref().map_or(1, |k| k.2.max(1)) as i64;
                let done = ((moved + per_step - 1) / per_step).min(i32::MAX as i64) as i32;
                self.in_flight = self.in_flight.signum() * (self.in_flight.abs() - done).max(0);
                self.last_progress = Some(now);
            }
        }
        self.position = Some(frame);
        true
    }

    /// Whether the loop should come back soon to keep things moving.
    pub fn busy(&self) -> bool {
        self.waiting != 0 || self.in_flight != 0 || self.query.is_some()
    }

    /// Forgets waiting steps, e.g. when the profile changes.
    pub fn clear(&mut self) {
        self.waiting = 0;
        self.in_flight = 0;
    }

    fn direct(&self, now: Instant) -> bool {
        self.direct_until.is_some_and(|t| now < t)
    }

    /// Resolve's playhead can't be read (scripting off, no timeline): send
    /// steps straight through for a while, as without pacing.
    pub fn fail(&mut self, now: Instant) {
        self.direct_until = Some(now + DIRECT);
        self.position = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> (Shortcut, Shortcut) {
        (Shortcut::parse("right").unwrap(), Shortcut::parse("left").unwrap())
    }

    #[test]
    fn keeps_only_a_few_steps_in_flight() {
        let (f, r) = keys();
        let t = Instant::now();
        let mut p = Pacer::default();
        p.add(10, f.clone(), r.clone(), 1, t);
        let (taps, query) = p.pump(t);
        assert!(taps.is_empty() && query, "waits for a first reading");
        p.queried(1, t);
        assert!(p.on_reply(Some(1), true, "100", t));
        let (taps, query) = p.pump(t);
        assert_eq!(taps.len(), MAX_IN_FLIGHT as usize);
        assert!(query);
        // The rest of the burst was dropped, not queued.
        p.queried(2, t);
        p.on_reply(Some(2), true, "101", t);
        assert!(p.pump(t).0.is_empty());
        // Resolve showed one of them, so the next turn may send one more.
        p.add(5, f, r, 1, t);
        assert_eq!(p.pump(t).0.len(), 1);
        p.queried(3, t);
        p.on_reply(Some(3), true, "103", t);
        assert_eq!(p.pump(t).0.len(), 1, "both seen, the waiting one goes");
        p.queried(4, t);
        p.on_reply(Some(4), true, "104", t);
        assert!(p.pump(t).0.is_empty());
        assert!(!p.busy());
    }

    #[test]
    fn falls_back_to_direct_steps_without_resolve() {
        let (f, r) = keys();
        let t = Instant::now();
        let mut p = Pacer::default();
        p.add(3, f.clone(), r.clone(), 1, t);
        p.pump(t);
        p.queried(1, t);
        p.on_reply(Some(1), false, "No timeline is open in Resolve", t);
        assert_eq!(p.pump(t).0.len(), 2);
        p.add(3, f, r, 1, t);
        assert_eq!(p.pump(t).0.len(), 3, "no pacing while Resolve can't be read");
    }

    #[test]
    fn stalled_steps_are_forgotten() {
        let (f, r) = keys();
        let t = Instant::now();
        let mut p = Pacer::default();
        p.add(1, f.clone(), r.clone(), 1, t);
        p.pump(t);
        p.queried(1, t);
        p.on_reply(Some(1), true, "0", t);
        assert_eq!(p.pump(t).0.len(), 1);
        p.query = None;
        let later = t + STALL + Duration::from_millis(1);
        p.add(1, f, r, 1, later);
        assert_eq!(p.pump(later).0.len(), 1, "the stuck step no longer blocks");
    }
}
