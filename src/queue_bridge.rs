//! F1A boundary: project legacy rows to pure queue rules without shadow state.
//!
//! The live engine, transport confirmation and profile persistence stay owned by
//! the legacy app. This bridge does not turn a Spotify URI into another source.

use crate::api::models::PlayableItem;
pub(crate) use doremi_core::queue::MovePlan;

pub(crate) fn manual_prefix(rows: &[PlayableItem], manual: &[String]) -> usize {
    doremi_core::queue::manual_prefix_len(
        rows.iter().map(PlayableItem::uri),
        manual.iter().map(String::as_str),
    )
}
