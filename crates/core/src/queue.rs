//! Play queue: the ordered list of tracks plus shuffle and repeat rules.
//!
//! Entries are addressed by a stable [`TrackId`] so the queue can be edited while a track plays.
//! The queue only answers "what comes next"; the engine decides when to ask.

use std::path::{Path, PathBuf};

/// Stable identifier for a queue entry. Never reused within one queue.
pub type TrackId = u64;

/// What happens when the end of the queue is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    /// Stop after the last track.
    #[default]
    Off,
    /// Wrap around to the first track.
    All,
    /// Repeat the current track forever.
    One,
}

/// One queued file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueEntry {
    pub id: TrackId,
    pub path: PathBuf,
}

/// Ordered list of tracks with shuffle and repeat.
#[derive(Debug, Clone)]
pub struct Queue {
    entries: Vec<QueueEntry>,
    next_id: TrackId,
    repeat: Repeat,
    /// Play order when shuffle is on. Contains every entry id exactly once.
    shuffle_order: Option<Vec<TrackId>>,
    rng: XorShift,
    /// Bumped on every change, so observers can skip work when nothing changed.
    version: u64,
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}

impl Queue {
    pub fn new() -> Self {
        Self::with_seed(0x9E37_79B9_7F4A_7C15)
    }

    /// Creates a queue whose shuffle order is reproducible from `seed`.
    pub fn with_seed(seed: u64) -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            repeat: Repeat::Off,
            shuffle_order: None,
            rng: XorShift::new(seed),
            version: 0,
        }
    }

    /// Changes whenever entries, order, shuffle or repeat change.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn entries(&self) -> &[QueueEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, id: TrackId) -> Option<&QueueEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Index of `id` in display order.
    pub fn position(&self, id: TrackId) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    /// Appends a track and returns its id.
    pub fn push(&mut self, path: impl AsRef<Path>) -> TrackId {
        self.version += 1;
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(QueueEntry {
            id,
            path: path.as_ref().to_path_buf(),
        });
        if let Some(order) = self.shuffle_order.as_mut() {
            let at = self.rng.below(order.len() + 1);
            order.insert(at, id);
        }
        id
    }

    pub fn extend<I, P>(&mut self, paths: I) -> Vec<TrackId>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        paths.into_iter().map(|p| self.push(p)).collect()
    }

    /// Removes a track. Returns false if it was not queued.
    pub fn remove(&mut self, id: TrackId) -> bool {
        self.version += 1;
        let Some(pos) = self.position(id) else {
            return false;
        };
        self.entries.remove(pos);
        if let Some(order) = self.shuffle_order.as_mut() {
            order.retain(|x| *x != id);
        }
        true
    }

    pub fn clear(&mut self) {
        self.version += 1;
        self.entries.clear();
        if let Some(order) = self.shuffle_order.as_mut() {
            order.clear();
        }
    }

    /// Moves the entry at display index `from` to display index `to`.
    pub fn move_entry(&mut self, from: usize, to: usize) -> bool {
        self.version += 1;
        if from >= self.entries.len() || to >= self.entries.len() {
            return false;
        }
        let entry = self.entries.remove(from);
        self.entries.insert(to, entry);
        true
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.version += 1;
        self.repeat = repeat;
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle_order.is_some()
    }

    /// Turns shuffle on or off. Turning it on draws a fresh random order.
    pub fn set_shuffle(&mut self, on: bool) {
        self.version += 1;
        if on {
            let mut order: Vec<TrackId> = self.entries.iter().map(|e| e.id).collect();
            self.rng.shuffle(&mut order);
            self.shuffle_order = Some(order);
        } else {
            self.shuffle_order = None;
        }
    }

    /// The ids in play order.
    pub fn play_order(&self) -> Vec<TrackId> {
        match &self.shuffle_order {
            Some(order) => order.clone(),
            None => self.entries.iter().map(|e| e.id).collect(),
        }
    }

    /// First track in play order.
    pub fn first(&self) -> Option<TrackId> {
        match &self.shuffle_order {
            Some(order) => order.first().copied(),
            None => self.entries.first().map(|e| e.id),
        }
    }

    /// Track to play after `id` finishes on its own. Honors [`Repeat::One`].
    pub fn after_finished(&self, id: TrackId) -> Option<TrackId> {
        if self.repeat == Repeat::One && self.get(id).is_some() {
            return Some(id);
        }
        self.next_after(id)
    }

    /// Track the "next" button goes to from `id`. Ignores [`Repeat::One`], like the classic player.
    /// If `id` is no longer queued, starts from the top.
    pub fn next_after(&self, id: TrackId) -> Option<TrackId> {
        let order = self.play_order();
        let Some(pos) = order.iter().position(|x| *x == id) else {
            return order.first().copied();
        };
        match order.get(pos + 1) {
            Some(next) => Some(*next),
            None if self.repeat != Repeat::Off => order.first().copied(),
            None => None,
        }
    }

    /// Track the "previous" button goes to from `id`.
    pub fn prev_before(&self, id: TrackId) -> Option<TrackId> {
        let order = self.play_order();
        let Some(pos) = order.iter().position(|x| *x == id) else {
            return order.first().copied();
        };
        if pos > 0 {
            Some(order[pos - 1])
        } else if self.repeat != Repeat::Off {
            order.last().copied()
        } else {
            Some(order[0])
        }
    }
}

/// Small deterministic PRNG for shuffle order. Not for anything security related.
#[derive(Debug, Clone)]
struct XorShift(u64);

impl XorShift {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x2545_F491_4F6C_DD1D
        } else {
            seed
        })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform-ish value in `0..n`. `n` must be non-zero.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue_of(n: usize) -> (Queue, Vec<TrackId>) {
        let mut q = Queue::with_seed(42);
        let ids = q.extend((0..n).map(|i| format!("/t/{i}.flac")));
        (q, ids)
    }

    #[test]
    fn ids_are_unique_and_stable_across_removal() {
        let (mut q, ids) = queue_of(3);
        assert!(q.remove(ids[1]));
        let new = q.push("/t/new.flac");
        assert!(!ids.contains(&new));
        assert_eq!(q.position(ids[2]), Some(1));
        assert!(!q.remove(ids[1]));
    }

    #[test]
    fn linear_order_without_repeat_stops_at_end() {
        let (q, ids) = queue_of(3);
        assert_eq!(q.first(), Some(ids[0]));
        assert_eq!(q.next_after(ids[0]), Some(ids[1]));
        assert_eq!(q.next_after(ids[2]), None);
        assert_eq!(q.after_finished(ids[2]), None);
    }

    #[test]
    fn repeat_all_wraps_both_ways() {
        let (mut q, ids) = queue_of(3);
        q.set_repeat(Repeat::All);
        assert_eq!(q.next_after(ids[2]), Some(ids[0]));
        assert_eq!(q.prev_before(ids[0]), Some(ids[2]));
    }

    #[test]
    fn prev_on_first_without_repeat_stays() {
        let (q, ids) = queue_of(3);
        assert_eq!(q.prev_before(ids[0]), Some(ids[0]));
        assert_eq!(q.prev_before(ids[2]), Some(ids[1]));
    }

    #[test]
    fn repeat_one_only_affects_natural_end() {
        let (mut q, ids) = queue_of(3);
        q.set_repeat(Repeat::One);
        assert_eq!(q.after_finished(ids[1]), Some(ids[1]));
        assert_eq!(q.next_after(ids[1]), Some(ids[2]));
    }

    #[test]
    fn removed_current_track_restarts_from_top() {
        let (mut q, ids) = queue_of(3);
        q.remove(ids[1]);
        assert_eq!(q.next_after(ids[1]), Some(ids[0]));
    }

    #[test]
    fn empty_queue_has_nothing_next() {
        let q = Queue::new();
        assert_eq!(q.first(), None);
        assert_eq!(q.next_after(7), None);
        assert_eq!(q.prev_before(7), None);
    }

    #[test]
    fn shuffle_is_a_permutation_and_reproducible() {
        let (mut a, ids) = queue_of(20);
        let (mut b, _) = queue_of(20);
        a.set_shuffle(true);
        b.set_shuffle(true);
        let order = a.play_order();
        assert_eq!(order, b.play_order());
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, ids);
        assert_ne!(
            order, ids,
            "20 items should not shuffle to identity with this seed"
        );
    }

    #[test]
    fn shuffle_walk_visits_every_track_once() {
        let (mut q, ids) = queue_of(10);
        q.set_shuffle(true);
        let mut seen = Vec::new();
        let mut cur = q.first();
        while let Some(id) = cur {
            seen.push(id);
            cur = q.next_after(id);
        }
        seen.sort();
        assert_eq!(seen, ids);
    }

    #[test]
    fn shuffle_tracks_pushes_and_removals() {
        let (mut q, ids) = queue_of(5);
        q.set_shuffle(true);
        let new = q.push("/t/extra.flac");
        q.remove(ids[0]);
        let order = q.play_order();
        assert_eq!(order.len(), 5);
        assert!(order.contains(&new));
        assert!(!order.contains(&ids[0]));
        q.set_shuffle(false);
        assert_eq!(q.play_order()[0], ids[1]);
    }

    #[test]
    fn version_changes_on_every_mutation() {
        let mut q = Queue::new();
        let v0 = q.version();
        let id = q.push("/a.flac");
        assert!(q.version() > v0);
        let v1 = q.version();
        let _ = q.entries();
        let _ = q.next_after(id);
        assert_eq!(q.version(), v1, "reads do not change it");
        q.set_shuffle(true);
        q.remove(id);
        assert!(q.version() > v1);
    }

    #[test]
    fn move_entry_reorders_display() {
        let (mut q, ids) = queue_of(3);
        assert!(q.move_entry(0, 2));
        assert_eq!(q.play_order(), vec![ids[1], ids[2], ids[0]]);
        assert!(!q.move_entry(5, 0));
    }
}
