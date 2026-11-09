//! Autosave timing: a note is written once its edits rest for [`QUIET`], and at least every
//! [`MAX_WAIT`] while they keep coming. Pure, with the clock passed in, so the rules are tested
//! without waiting.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::model::NoteId;

pub const QUIET: Duration = Duration::from_millis(500);
pub const MAX_WAIT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub text: Arc<str>,
    /// The store's edit generation, echoed back once written.
    pub generation: u64,
    first: Instant,
    last: Instant,
}

impl Pending {
    fn due_at(&self) -> Instant {
        (self.last + QUIET).min(self.first + MAX_WAIT)
    }
}

#[derive(Debug, Default)]
pub struct Debouncer {
    pending: HashMap<NoteId, Pending>,
}

impl Debouncer {
    /// Records the newest text of `note` at `now`.
    pub fn push(&mut self, note: NoteId, text: Arc<str>, generation: u64, now: Instant) {
        let first = self.pending.get(&note).map_or(now, |p| p.first);
        self.pending.insert(note, Pending { text, generation, first, last: now });
    }

    /// The earliest time something is due.
    pub fn next_due(&self) -> Option<Instant> {
        self.pending.values().map(Pending::due_at).min()
    }

    /// Takes everything due at `now`.
    pub fn take_due(&mut self, now: Instant) -> Vec<(NoteId, Pending)> {
        let due: Vec<NoteId> =
            self.pending.iter().filter(|(_, p)| p.due_at() <= now).map(|(id, _)| id.clone()).collect();
        due.into_iter().filter_map(|id| self.pending.remove(&id).map(|p| (id, p))).collect()
    }

    /// Takes the notes at or under `path`, due or not: they are about to move.
    pub fn take_under(&mut self, path: &str) -> Vec<(NoteId, Pending)> {
        let hit: Vec<NoteId> =
            self.pending.keys().filter(|id| super::model::rebase(&id.0, path, "").is_some()).cloned().collect();
        hit.into_iter().filter_map(|id| self.pending.remove(&id).map(|p| (id, p))).collect()
    }

    /// Takes everything.
    pub fn take_all(&mut self) -> Vec<(NoteId, Pending)> {
        self.pending.drain().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> NoteId {
        NoteId("a.md".into())
    }

    #[test]
    fn a_save_waits_for_the_edits_to_rest() {
        let t0 = Instant::now(); // nd-allow(clock-in-ui): a fixed origin for test times
        let mut d = Debouncer::default();
        d.push(id(), "a".into(), 1, t0);
        d.push(id(), "ab".into(), 2, t0 + Duration::from_millis(300));
        assert!(d.take_due(t0 + Duration::from_millis(700)).is_empty(), "300 ms after the last edit");
        let due = d.take_due(t0 + Duration::from_millis(800));
        assert_eq!(due.len(), 1);
        assert_eq!((&*due[0].1.text, due[0].1.generation), ("ab", 2), "the newest text is written once");
        assert_eq!(d.next_due(), None);
    }

    #[test]
    fn steady_typing_still_saves_every_two_seconds() {
        let t0 = Instant::now(); // nd-allow(clock-in-ui): a fixed origin for test times
        let mut d = Debouncer::default();
        for ms in (0..=1900).step_by(100) {
            d.push(id(), ms.to_string().into(), ms, t0 + Duration::from_millis(ms));
        }
        assert_eq!(d.next_due(), Some(t0 + MAX_WAIT), "capped by the first unsaved edit");
        assert_eq!(d.take_due(t0 + MAX_WAIT).len(), 1);
    }

    #[test]
    fn a_move_takes_the_notes_under_it() {
        let t0 = Instant::now(); // nd-allow(clock-in-ui): a fixed origin for test times
        let mut d = Debouncer::default();
        d.push(NoteId("Work/a.md".into()), "x".into(), 1, t0);
        d.push(NoteId("Workshop.md".into()), "y".into(), 2, t0);
        assert_eq!(d.take_under("Work").len(), 1);
        assert_eq!(d.take_all().len(), 1);
    }
}
