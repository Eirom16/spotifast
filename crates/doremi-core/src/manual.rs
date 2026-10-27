//! One borrowed authority for manual removals and their confirmation policy.
//! No shadow queue: the caller's Vec is edited in place in one synchronous step.

use std::collections::BTreeSet;

use crate::requests::{Router, Ticket};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Removal {
    visible: Vec<usize>,
    manual: Vec<usize>,
}

impl Removal {
    pub fn visible_indices(&self) -> &[usize] {
        &self.visible
    }

    pub fn is_empty(&self) -> bool {
        self.manual.is_empty()
    }

    /// A pending addition follows its occurrence, or disappears with that occurrence.
    pub fn remap(&self, old: usize) -> Option<usize> {
        if self.manual.binary_search(&old).is_ok() {
            return None;
        }
        Some(old - self.manual.partition_point(|removed| *removed < old))
    }
}

/// Legacy selection carries index + key, not a global occurrence ID. Match the
/// ordinal among identical visible keys to the same ordinal in the manual list.
/// Stale/missing rows and context rows are ignored; duplicates are not a set.
pub fn remove_selected<T: Eq>(
    manual: &mut Vec<T>,
    visible: &[T],
    prefix: usize,
    selection: &[(usize, T)],
) -> Removal {
    let indexes: BTreeSet<_> = selection
        .iter()
        .filter(|(index, key)| *index < prefix && visible.get(*index) == Some(key))
        .map(|(index, _)| *index)
        .collect();
    let mut result = Removal::default();
    for index in indexes {
        let key = &visible[index];
        let occurrence = visible[..index]
            .iter()
            .filter(|other| *other == key)
            .count();
        if let Some(manual_index) = manual
            .iter()
            .enumerate()
            .filter(|(_, other)| *other == key)
            .nth(occurrence)
            .map(|(index, _)| index)
        {
            result.visible.push(index);
            result.manual.push(manual_index);
        }
    }
    result.manual.sort_unstable();
    result.manual.dedup();
    for &index in result.manual.iter().rev() {
        manual.remove(index);
    }
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmationDecision {
    Obsolete,
    Retry,
    Confirmed,
    /// Preserve the legacy bounded fallback, not infinite optimistic ownership.
    Fallback,
}

/// Only a request fence and policy, never another copy of the authoritative rows.
#[derive(Clone, Copy, Debug)]
pub struct Confirmation {
    ticket: Ticket,
    hold_ms: u64,
    retry_limit: u8,
}

impl Confirmation {
    pub fn new(ticket: Ticket, hold_ms: u64, retry_limit: u8) -> Self {
        Self {
            ticket,
            hold_ms,
            retry_limit,
        }
    }

    pub fn decide<T: PartialEq>(
        self,
        router: &Router,
        elapsed_ms: u64,
        retries: u8,
        expected: impl IntoIterator<Item = T>,
        incoming: impl IntoIterator<Item = T>,
    ) -> ConfirmationDecision {
        if !router.accepts(self.ticket) {
            return ConfirmationDecision::Obsolete;
        }
        if expected.into_iter().eq(incoming) {
            return ConfirmationDecision::Confirmed;
        }
        if elapsed_ms >= self.hold_ms || retries >= self.retry_limit {
            ConfirmationDecision::Fallback
        } else {
            ConfirmationDecision::Retry
        }
    }
}
