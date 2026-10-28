//! Portable checkpoint data, with transactional validation at the domain boundary.
//! Hosts own versioned encoding and I/O. This record is a backup, never a second
//! live queue. Restoring uses the existing reducer and always resumes paused.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub const ENTRY_LIMIT: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryRecord {
    pub id: u64,
    pub media: Media,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub entry: EntryRecord,
    pub context_index: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub current: Option<Occurrence>,
    pub manual: Vec<EntryRecord>,
    pub context: Vec<EntryRecord>,
    pub order: Vec<usize>,
    pub cursor: Option<usize>,
    pub history: Vec<Occurrence>,
    pub forward: Vec<Occurrence>,
    pub repeat: Repeat,
    pub shuffled: bool,
    pub position_ms: u64,
    pub next_entry: u64,
    pub request_watermark: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    TooLarge,
    InvalidOccurrence,
    InvalidOrder,
    InvalidCursor,
    InvalidPosition,
    Reducer(QueueError),
}

impl From<Entry> for EntryRecord {
    fn from(entry: Entry) -> Self {
        Self {
            id: entry.id.0,
            media: entry.media,
        }
    }
}
impl From<EntryRecord> for Entry {
    fn from(entry: EntryRecord) -> Self {
        Self {
            id: EntryId(entry.id),
            media: entry.media,
        }
    }
}
impl From<Current> for Occurrence {
    fn from(current: Current) -> Self {
        Self {
            entry: current.entry.into(),
            context_index: current.context_index,
        }
    }
}
impl From<Occurrence> for Current {
    fn from(current: Occurrence) -> Self {
        Self {
            entry: current.entry.into(),
            context_index: current.context_index,
        }
    }
}

impl Checkpoint {
    pub fn validate(&self) -> Result<(), CheckpointError> {
        use CheckpointError::*;
        if self.manual.len() > ENTRY_LIMIT
            || self.context.len() > ENTRY_LIMIT
            || self.order.len() > ENTRY_LIMIT
            || self.history.len() > HISTORY_LIMIT
            || self.forward.len() > HISTORY_LIMIT
        {
            return Err(TooLarge);
        }
        if Queue::order(&Order::Shuffled(self.order.clone()), self.context.len()).is_err()
            || (!self.shuffled && self.order.iter().copied().ne(0..self.context.len()))
        {
            return Err(InvalidOrder);
        }
        if self.cursor.is_some_and(|cursor| cursor >= self.order.len())
            || (self.context.is_empty() != self.cursor.is_none())
        {
            return Err(InvalidCursor);
        }
        let context_ids: BTreeSet<_> = self.context.iter().map(|entry| entry.id).collect();
        let manual_ids: BTreeSet<_> = self.manual.iter().map(|entry| entry.id).collect();
        if context_ids.len() != self.context.len()
            || manual_ids.len() != self.manual.len()
            || !context_ids.is_disjoint(&manual_ids)
        {
            return Err(InvalidOccurrence);
        }
        let mut known = BTreeMap::new();
        let mut register = |entry: &EntryRecord| -> Result<(), CheckpointError> {
            if entry.id >= self.next_entry
                || entry.media.id.provider.as_str().len() > 64
                || entry.media.id.local().len() > 1024
                || entry.media.title.len() > 4096
            {
                return Err(InvalidOccurrence);
            }
            if let Some(media) = known.insert(entry.id, entry.media.clone())
                && media != entry.media
            {
                return Err(InvalidOccurrence);
            }
            Ok(())
        };
        for entry in self.context.iter().chain(&self.manual) {
            register(entry)?;
        }
        let mut heard_manual = BTreeSet::new();
        for occurrence in self
            .current
            .iter()
            .chain(&self.history)
            .chain(&self.forward)
        {
            register(&occurrence.entry)?;
            match occurrence.context_index {
                Some(index) if self.context.get(index) == Some(&occurrence.entry) => {}
                Some(_) => return Err(InvalidOccurrence),
                None if context_ids.contains(&occurrence.entry.id)
                    || manual_ids.contains(&occurrence.entry.id)
                    || !heard_manual.insert(occurrence.entry.id) =>
                {
                    return Err(InvalidOccurrence);
                }
                None => {}
            }
        }
        if let Some(index) = self.current.as_ref().and_then(|c| c.context_index)
            && self
                .cursor
                .and_then(|cursor| self.order.get(cursor))
                .copied()
                != Some(index)
        {
            return Err(InvalidCursor);
        }
        match &self.current {
            Some(current)
                if current
                    .entry
                    .media
                    .duration_ms
                    .is_some_and(|duration| self.position_ms > duration) =>
            {
                return Err(InvalidPosition);
            }
            _ => {}
        }
        Ok(())
    }
    fn into_snapshot(self) -> Result<Snapshot, CheckpointError> {
        self.validate()?;
        Ok(Snapshot {
            current: self.current.map(Into::into),
            manual: self.manual.into_iter().map(Into::into).collect(),
            context: self.context.into_iter().map(Into::into).collect(),
            order: self.order,
            cursor: self.cursor,
            history: self.history.into_iter().map(Into::into).collect(),
            forward: self.forward.into_iter().map(Into::into).collect(),
            repeat: self.repeat,
            shuffled: self.shuffled,
            position_ms: self.position_ms,
            next_entry: self.next_entry,
            request_watermark: self.request_watermark,
        })
    }
}
impl Queue {
    pub fn checkpoint(&self) -> Checkpoint {
        let s = self.snapshot();
        Checkpoint {
            current: s.current.map(Into::into),
            manual: s.manual.into_iter().map(Into::into).collect(),
            context: s.context.into_iter().map(Into::into).collect(),
            order: s.order,
            cursor: s.cursor,
            history: s.history.into_iter().map(Into::into).collect(),
            forward: s.forward.into_iter().map(Into::into).collect(),
            repeat: s.repeat,
            shuffled: s.shuffled,
            position_ms: s.position_ms,
            next_entry: s.next_entry,
            request_watermark: s.request_watermark,
        }
    }
    pub fn restore_checkpoint(
        &mut self,
        checkpoint: Checkpoint,
    ) -> Result<Vec<Effect>, CheckpointError> {
        let snapshot = checkpoint.into_snapshot()?;
        self.command(Command::Restore(Box::new(snapshot)))
            .map_err(CheckpointError::Reducer)
    }
}
