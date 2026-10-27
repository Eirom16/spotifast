//! Borrowed manual-edit boundary; no second queue, transport or profile store.
//!
//! The live engine, transport confirmation and profile persistence stay owned by
//! the legacy app. This bridge does not turn a Spotify URI into another source.

use crate::api::models::PlayableItem;
use doremi_core::manual::{Confirmation, ConfirmationDecision, Removal};
pub(crate) use doremi_core::queue::MovePlan;
use doremi_core::requests::{Lane, Router, SessionId};
use std::time::Instant;

pub(crate) struct PendingEdit {
    started: Instant,
    confirmation: Confirmation,
}

impl PendingEdit {
    pub(crate) fn new(router: &mut Router, hold_ms: u64, retries: u8) -> Self {
        let (ticket, _) = router
            .begin(Lane::ManualEdit)
            .expect("manual generation exhausted");
        Self {
            started: Instant::now(),
            confirmation: Confirmation::new(ticket, hold_ms, retries),
        }
    }

    pub(crate) fn is_stale(
        &self,
        router: &Router,
        retries: u8,
        rows: &[PlayableItem],
        manual: &[String],
    ) -> bool {
        matches!(
            self.confirmation.decide(
                router,
                self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                retries,
                manual.iter().map(String::as_str),
                rows.iter().take(manual.len()).map(PlayableItem::uri),
            ),
            ConfirmationDecision::Retry | ConfirmationDecision::Obsolete
        )
    }
}

pub(crate) fn replace_session(router: &mut Router) {
    let session = router
        .session()
        .0
        .checked_add(1)
        .expect("queue session exhausted");
    router
        .replace_session(SessionId(session))
        .expect("new queue session");
}

pub(crate) fn remove(
    manual: &mut Vec<String>,
    rows: &[PlayableItem],
    prefix: usize,
    selected: &[(usize, String)],
) -> Removal {
    // Short-lived wire projection, not an authoritative queue.
    let visible: Vec<_> = rows.iter().map(|item| item.uri().to_owned()).collect();
    doremi_core::manual::remove_selected(manual, &visible, prefix, selected)
}

pub(crate) fn manual_prefix(rows: &[PlayableItem], manual: &[String]) -> usize {
    doremi_core::queue::manual_prefix_len(
        rows.iter().map(PlayableItem::uri),
        manual.iter().map(String::as_str),
    )
}
