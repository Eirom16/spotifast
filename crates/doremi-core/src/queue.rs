//! Deterministic queue domain. No clocks, randomness, serialization, threads or I/O.
//!
//! A manual occurrence plays once, ahead of the context continuation. Shuffle is
//! an explicit permutation supplied by the caller, so replay never depends on an
//! OS RNG. Each load has a distinct request ID, including replays of one entry.
//! Snapshots are in-memory domain values, NOT a replacement for legacy profiles.

pub mod checkpoint;

use std::collections::BTreeMap;

use crate::media::Media;
use crate::requests::SessionId;

/// Bounded heard history; not a promise of unlimited engine history.
pub const HISTORY_LIMIT: usize = 256;

/// A drag destination is a gap in the list *before* removal, including its end.
/// Shared with the legacy bridge; pending row indexes must follow the same move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MovePlan {
    pub from: usize,
    pub destination: usize,
}

impl MovePlan {
    pub fn new(len: usize, from: usize, gap: usize) -> Option<Self> {
        if from >= len || gap > len || gap == from || gap == from + 1 {
            return None;
        }
        Some(Self {
            from,
            destination: if gap > from { gap - 1 } else { gap },
        })
    }

    pub fn remap(self, index: usize) -> usize {
        if index == self.from {
            self.destination
        } else if self.from < index && index <= self.destination {
            index - 1
        } else if self.destination <= index && index < self.from {
            index + 1
        } else {
            index
        }
    }

    pub fn apply<T>(self, rows: &mut Vec<T>) {
        // Plans are public values: invalid caller input must not panic.
        if self.from < rows.len() && self.destination < rows.len() {
            let row = rows.remove(self.from);
            rows.insert(self.destination, row);
        }
    }
}

/// Match only the contiguous manual prefix, with multiplicity, not a set.
/// A matching context copy after the prefix must not become a manual occurrence.
pub fn manual_prefix_len<K: Ord>(
    rows: impl IntoIterator<Item = K>,
    manual: impl IntoIterator<Item = K>,
) -> usize {
    let mut counts = BTreeMap::new();
    for key in manual {
        *counts.entry(key).or_insert(0_usize) += 1;
    }
    rows.into_iter()
        .take_while(|key| match counts.get_mut(key) {
            Some(left) if *left > 0 => {
                *left -= 1;
                true
            }
            _ => false,
        })
        .count()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntryId(u64);

impl EntryId {
    /// Host-visible occurrence label, not a media ID or a cross-session identity.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestId {
    session: SessionId,
    generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: EntryId,
    pub media: Media,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    #[default]
    Off,
    Context,
    Track,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Playback {
    #[default]
    Stopped,
    Loading,
    Playing,
    Paused,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    Sequential,
    /// Exactly one of each canonical context index, not a partial result.
    Shuffled(Vec<usize>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Current {
    entry: Entry,
    context_index: Option<usize>,
}

/// Owned snapshot; its invariants cannot be modified externally.
/// Restoration pauses and invalidates in-flight work. Hosts own versioned encoding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    current: Option<Current>,
    manual: Vec<Entry>,
    context: Vec<Entry>,
    order: Vec<usize>,
    cursor: Option<usize>,
    history: Vec<Current>,
    forward: Vec<Current>,
    repeat: Repeat,
    shuffled: bool,
    position_ms: u64,
    next_entry: u64,
    request_watermark: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Append(Vec<Media>),
    Insert {
        gap: usize,
        media: Vec<Media>,
    },
    Move {
        entry: EntryId,
        gap: usize,
    },
    Remove(EntryId),
    ClearManual,
    StartContext {
        media: Vec<Media>,
        start: usize,
        order: Order,
    },
    SetOrder(Order),
    SetRepeat(Repeat),
    Next,
    Previous,
    Play,
    Pause,
    Seek(u64),
    /// Host starts an asynchronous, all-or-nothing album/context read.
    BeginAppend,
    /// Cancel outstanding work, stopping audio but retaining a paused resume point.
    Cancel,
    Restore(Box<Snapshot>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Ready(RequestId),
    Position {
        request: RequestId,
        position_ms: u64,
    },
    Ended(RequestId),
    Failed(RequestId),
    Appended {
        request: RequestId,
        media: Vec<Media>,
    },
    AppendFailed(RequestId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Prepare this occurrence; publish audio only after matching Ready is
    /// accepted and SetPlaying supplies the latest (possibly paused) intent.
    Load {
        request: RequestId,
        entry: Entry,
        position_ms: u64,
        playing: bool,
    },
    Cancel(RequestId),
    SetPlaying(bool),
    Stop,
    ReadAppend(RequestId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueError {
    InvalidIndex,
    InvalidPermutation,
    UnknownEntry,
    Exhausted,
    SessionNotNewer,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Queue {
    snapshot: Snapshot,
    playback: Playback,
    playing: bool,
    loaded: bool,
    request: Option<RequestId>,
    append: Option<RequestId>,
    serial: u64,
    session: SessionId,
}

impl Queue {
    /// Hosts allocate a fresh session when replacing a Queue with live callbacks.
    /// Default (session zero) remains useful for isolated deterministic fixtures.
    pub fn new(session: SessionId) -> Self {
        Self {
            session,
            ..Self::default()
        }
    }

    pub fn replace_session(&mut self, session: SessionId) -> Result<Vec<Effect>, QueueError> {
        if session <= self.session {
            return Err(QueueError::SessionNotNewer);
        }
        let effects = self.command(Command::Cancel)?;
        self.session = session;
        Ok(effects)
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.snapshot.clone();
        snapshot.request_watermark = self.serial;
        snapshot
    }

    pub fn current(&self) -> Option<&Entry> {
        self.snapshot.current.as_ref().map(|c| &c.entry)
    }

    pub fn manual(&self) -> &[Entry] {
        &self.snapshot.manual
    }

    pub fn context(&self) -> &[Entry] {
        &self.snapshot.context
    }

    pub fn upcoming(&self) -> impl Iterator<Item = &Entry> {
        let s = &self.snapshot;
        let cursor = s
            .forward
            .iter()
            .find_map(|c| c.context_index)
            .and_then(|index| s.order.iter().position(|held| *held == index))
            .or(s.cursor);
        let start = cursor.map_or(0, |cursor| cursor + 1);
        s.forward
            .iter()
            .rev()
            .map(|c| &c.entry)
            .chain(s.manual.iter())
            .chain(s.order[start..].iter().map(|index| &s.context[*index]))
    }

    pub fn playback(&self) -> Playback {
        self.playback
    }

    pub fn position_ms(&self) -> u64 {
        self.snapshot.position_ms
    }

    pub fn repeat(&self) -> Repeat {
        self.snapshot.repeat
    }

    pub fn shuffled(&self) -> bool {
        self.snapshot.shuffled
    }

    fn order(order: &Order, len: usize) -> Result<Vec<usize>, QueueError> {
        match order {
            Order::Sequential => Ok((0..len).collect()),
            Order::Shuffled(order) => {
                let mut seen = vec![false; len];
                if order.len() != len {
                    return Err(QueueError::InvalidPermutation);
                }
                for &index in order {
                    let slot = seen.get_mut(index).ok_or(QueueError::InvalidPermutation)?;
                    if std::mem::replace(slot, true) {
                        return Err(QueueError::InvalidPermutation);
                    }
                }
                Ok(order.clone())
            }
        }
    }

    fn entries(&mut self, media: Vec<Media>) -> Vec<Entry> {
        media
            .into_iter()
            .map(|media| {
                let id = EntryId(self.snapshot.next_entry);
                self.snapshot.next_entry += 1; // checked before changing any state
                Entry { id, media }
            })
            .collect()
    }

    fn cancel_append(&mut self, effects: &mut Vec<Effect>) {
        if let Some(request) = self.append.take() {
            effects.push(Effect::Cancel(request));
        }
    }

    fn cancel_load(&mut self, effects: &mut Vec<Effect>) {
        if let Some(request) = self.request.take() {
            effects.push(Effect::Cancel(request));
        }
        self.loaded = false;
    }

    fn load(&mut self, effects: &mut Vec<Effect>) {
        // A restored/cancelled paused bookmark has no live decoder. Browsing
        // or seeking it does not start provider work until explicit Play.
        if !self.loaded
            && self.request.is_none()
            && !self.playing
            && self.playback == Playback::Paused
            && self.current().is_some()
        {
            return;
        }
        self.cancel_load(effects);
        if let Some(entry) = self.current().cloned() {
            let request = RequestId {
                session: self.session,
                generation: self.serial,
            };
            self.request = Some(request);
            self.playback = Playback::Loading;
            effects.push(Effect::Load {
                request,
                entry,
                position_ms: self.snapshot.position_ms,
                playing: self.playing,
            });
        } else {
            self.playback = Playback::Stopped;
            effects.push(Effect::Stop);
        }
    }

    fn select(&mut self, current: Current, effects: &mut Vec<Effect>) {
        if let Some(index) = current.context_index {
            self.snapshot.cursor = self.snapshot.order.iter().position(|held| *held == index);
        }
        self.snapshot.current = Some(current);
        self.snapshot.position_ms = 0;
        self.load(effects);
    }

    fn advance(&mut self, ended: bool, effects: &mut Vec<Effect>) {
        if ended && self.snapshot.repeat == Repeat::Track && self.current().is_some() {
            self.snapshot.position_ms = 0;
            self.load(effects);
            return;
        }
        let next = if let Some(forward) = self.snapshot.forward.pop() {
            Some(forward)
        } else if !self.snapshot.manual.is_empty() {
            Some(Current {
                entry: self.snapshot.manual.remove(0),
                context_index: None,
            })
        } else {
            let s = &mut self.snapshot;
            let next = s.cursor.map_or(0, |cursor| cursor + 1);
            let next = if next >= s.order.len() && s.repeat == Repeat::Context {
                0
            } else {
                next
            };
            s.order.get(next).map(|&index| Current {
                entry: s.context[index].clone(),
                context_index: Some(index),
            })
        };
        if let Some(current) = self.snapshot.current.take() {
            self.snapshot.history.push(current);
            if self.snapshot.history.len() > HISTORY_LIMIT {
                self.snapshot.history.remove(0);
            }
        }
        if let Some(next) = next {
            self.select(next, effects);
        } else {
            self.snapshot.position_ms = 0;
            self.load(effects);
        }
    }

    pub fn command(&mut self, command: Command) -> Result<Vec<Effect>, QueueError> {
        // Validate before changing anything: rejected input is transactional.
        let count = match &command {
            Command::Append(media)
            | Command::Insert { media, .. }
            | Command::StartContext { media, .. } => media.len(),
            _ => 0,
        };
        self.snapshot
            .next_entry
            .checked_add(count as u64)
            .ok_or(QueueError::Exhausted)?;
        let serial = match &command {
            Command::Restore(snapshot) => self.serial.max(snapshot.request_watermark),
            _ => self.serial,
        }
        .checked_add(1)
        .ok_or(QueueError::Exhausted)?;
        match &command {
            Command::Insert { gap, .. } if *gap > self.manual().len() => {
                return Err(QueueError::InvalidIndex);
            }
            Command::Move { entry, gap } => {
                if !self.manual().iter().any(|item| item.id == *entry) {
                    return Err(QueueError::UnknownEntry);
                }
                if *gap > self.manual().len() {
                    return Err(QueueError::InvalidIndex);
                }
            }
            Command::Remove(id) if !self.manual().iter().any(|item| item.id == *id) => {
                return Err(QueueError::UnknownEntry);
            }
            Command::StartContext {
                media,
                start,
                order,
            } => {
                if *start >= media.len() {
                    return Err(QueueError::InvalidIndex);
                }
                Self::order(order, media.len())?;
            }
            Command::SetOrder(order) => {
                Self::order(order, self.snapshot.context.len())?;
            }
            _ => {}
        }
        self.serial = serial;
        let changes_song = matches!(&command, Command::Next | Command::StartContext { .. })
            || (matches!(&command, Command::Previous) && self.snapshot.position_ms <= 3_000);
        if changes_song && self.snapshot.repeat == Repeat::Track {
            // Current legacy contract (#1022), not the older "keep Repeat One"
            // wording in the historical guide. Natural end/seek/restart keep it.
            self.snapshot.repeat = Repeat::Context;
        }
        let mut effects = Vec::new();
        match command {
            Command::Append(media) => {
                let entries = self.entries(media);
                self.snapshot.manual.extend(entries);
            }
            Command::Insert { gap, media } => {
                let entries = self.entries(media);
                self.snapshot.manual.splice(gap..gap, entries);
            }
            Command::Move { entry, gap } => {
                let from = self
                    .manual()
                    .iter()
                    .position(|item| item.id == entry)
                    .expect("validated");
                if let Some(plan) = MovePlan::new(self.manual().len(), from, gap) {
                    plan.apply(&mut self.snapshot.manual);
                }
            }
            Command::Remove(id) => self.snapshot.manual.retain(|item| item.id != id),
            Command::ClearManual => {
                self.snapshot.manual.clear();
                self.cancel_append(&mut effects);
            }
            Command::StartContext {
                media,
                start,
                order,
            } => {
                self.cancel_append(&mut effects);
                self.snapshot.order = Self::order(&order, media.len())?;
                self.snapshot.shuffled = matches!(order, Order::Shuffled(_));
                self.snapshot.context = self.entries(media);
                self.snapshot.history.clear();
                self.snapshot.forward.clear();
                self.playing = true;
                self.select(
                    Current {
                        entry: self.snapshot.context[start].clone(),
                        context_index: Some(start),
                    },
                    &mut effects,
                );
            }
            Command::SetOrder(order) => {
                let anchor = self
                    .snapshot
                    .cursor
                    .map(|cursor| self.snapshot.order[cursor]);
                self.snapshot.order = Self::order(&order, self.snapshot.context.len())?;
                self.snapshot.shuffled = matches!(order, Order::Shuffled(_));
                self.snapshot.cursor =
                    anchor.and_then(|anchor| self.snapshot.order.iter().position(|i| *i == anchor));
            }
            Command::SetRepeat(repeat) => self.snapshot.repeat = repeat,
            Command::Next => self.advance(false, &mut effects),
            Command::Previous => {
                if self.snapshot.position_ms > 3_000 {
                    self.snapshot.position_ms = 0;
                    self.load(&mut effects);
                } else if let Some(previous) = self.snapshot.history.pop() {
                    if let Some(current) = self.snapshot.current.take() {
                        self.snapshot.forward.push(current);
                    }
                    self.select(previous, &mut effects);
                } else if let Some(cursor) = self.snapshot.cursor.filter(|cursor| *cursor > 0) {
                    let index = self.snapshot.order[cursor - 1];
                    self.select(
                        Current {
                            entry: self.snapshot.context[index].clone(),
                            context_index: Some(index),
                        },
                        &mut effects,
                    );
                } else if self.current().is_some() {
                    self.snapshot.position_ms = 0;
                    self.load(&mut effects);
                }
            }
            Command::Play => {
                self.playing = true;
                if self.loaded {
                    self.playback = Playback::Playing;
                    effects.push(Effect::SetPlaying(true));
                } else if self.current().is_some() {
                    self.load(&mut effects);
                }
            }
            Command::Pause => {
                self.playing = false;
                if self.loaded {
                    self.playback = Playback::Paused;
                }
                effects.push(Effect::SetPlaying(false));
            }
            Command::Seek(position) => {
                self.snapshot.position_ms = self
                    .current()
                    .and_then(|e| e.media.duration_ms)
                    .map_or(position, |duration| position.min(duration));
                if self.current().is_some() {
                    self.load(&mut effects);
                }
            }
            Command::BeginAppend => {
                self.cancel_append(&mut effects);
                let request = RequestId {
                    session: self.session,
                    generation: self.serial,
                };
                self.append = Some(request);
                effects.push(Effect::ReadAppend(request));
            }
            Command::Cancel => {
                self.cancel_append(&mut effects);
                self.cancel_load(&mut effects);
                self.playing = false;
                self.playback = if self.current().is_some() {
                    Playback::Paused
                } else {
                    Playback::Stopped
                };
                effects.push(Effect::Stop);
            }
            Command::Restore(snapshot) => {
                self.cancel_append(&mut effects);
                self.cancel_load(&mut effects);
                // Never recycle an occurrence ID, even restoring an older checkpoint.
                let next_entry = self.snapshot.next_entry.max(snapshot.next_entry);
                self.snapshot = *snapshot;
                self.snapshot.next_entry = next_entry;
                self.playing = false;
                self.playback = if self.current().is_some() {
                    Playback::Paused
                } else {
                    Playback::Stopped
                };
                effects.push(Effect::Stop);
            }
        }
        Ok(effects)
    }

    pub fn event(&mut self, event: Event) -> Result<Vec<Effect>, QueueError> {
        match event {
            Event::Appended { request, media } if self.append == Some(request) => {
                // Check capacity before consuming the pending request.
                self.snapshot
                    .next_entry
                    .checked_add(media.len() as u64)
                    .ok_or(QueueError::Exhausted)?;
                let entries = self.entries(media);
                self.snapshot.manual.extend(entries);
                self.append = None;
            }
            Event::AppendFailed(request) if self.append == Some(request) => self.append = None,
            Event::Ready(request)
                if self.request == Some(request) && self.playback == Playback::Loading =>
            {
                self.loaded = true;
                self.playback = if self.playing {
                    Playback::Playing
                } else {
                    Playback::Paused
                };
                return Ok(vec![Effect::SetPlaying(self.playing)]);
            }
            Event::Position {
                request,
                position_ms,
            } if self.request == Some(request) && self.loaded => {
                self.snapshot.position_ms = self
                    .current()
                    .and_then(|e| e.media.duration_ms)
                    .map_or(position_ms, |duration| position_ms.min(duration));
            }
            Event::Failed(request) if self.request == Some(request) => {
                self.loaded = false;
                self.request = None;
                self.playback = Playback::Failed;
                return Ok(vec![Effect::Stop]);
            }
            Event::Ended(request) if self.request == Some(request) && self.loaded => {
                self.serial = self.serial.checked_add(1).ok_or(QueueError::Exhausted)?;
                let mut effects = Vec::new();
                self.advance(true, &mut effects);
                return Ok(effects);
            }
            _ => {} // Cancelled, superseded or duplicate reply: no mutation.
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{MediaId, ProviderId};

    fn song() -> Media {
        Media {
            id: MediaId::new(ProviderId::new("fixture").unwrap(), "a").unwrap(),
            title: "a".into(),
            duration_ms: None,
        }
    }

    #[test]
    fn repeated_context_history_is_bounded_and_replay_is_deterministic() {
        let mut a = Queue::default();
        a.command(Command::StartContext {
            media: vec![song()],
            start: 0,
            order: Order::Sequential,
        })
        .unwrap();
        a.command(Command::SetRepeat(Repeat::Context)).unwrap();
        let mut b = a.clone();
        for _ in 0..HISTORY_LIMIT * 3 {
            assert_eq!(a.command(Command::Next), b.command(Command::Next));
            assert_eq!(a, b);
        }
        assert_eq!(a.snapshot.history.len(), HISTORY_LIMIT);
    }

    #[test]
    fn counter_exhaustion_rejects_before_mutating_a_queue() {
        let mut q = Queue::default();
        q.snapshot.next_entry = u64::MAX;
        let before = q.clone();
        assert_eq!(
            q.command(Command::Append(vec![song()])),
            Err(QueueError::Exhausted)
        );
        assert_eq!(q, before);
        q.serial = u64::MAX;
        let before = q.clone();
        assert_eq!(q.command(Command::Cancel), Err(QueueError::Exhausted));
        assert_eq!(q, before);
    }
}
