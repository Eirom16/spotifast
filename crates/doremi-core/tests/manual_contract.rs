use doremi_core::manual::{Confirmation, ConfirmationDecision as D, remove_selected};
use doremi_core::requests::{Lane, Router, SessionId};

#[test]
fn removal_preserves_duplicate_occurrences_context_and_pending_indexes() {
    let mut authority = vec!["b", "x", "b", "y"];
    let shown = ["x", "b", "b", "y", "b", "context"];
    let edit = remove_selected(&mut authority, &shown, 4, &[(2, "b"), (2, "b"), (4, "b")]);
    assert_eq!(authority, ["b", "x", "y"]);
    assert_eq!(edit.visible_indices(), [2]);
    assert_eq!(
        (0..4).map(|i| edit.remap(i)).collect::<Vec<_>>(),
        [Some(0), Some(1), None, Some(2)]
    );
}

#[test]
fn simultaneous_visible_and_manual_orders_may_differ() {
    let mut authority = vec!["a", "b", "c", "b"];
    let shown = ["b", "c", "a", "b", "context"];
    let edit = remove_selected(&mut authority, &shown, 4, &[(2, "a"), (0, "b")]);
    assert_eq!(authority, ["c", "b"]);
    assert_eq!(edit.visible_indices(), [0, 2]);
    assert_eq!(edit.remap(3), Some(1));
}

#[test]
fn stale_selection_does_not_remove_the_same_key_from_another_row() {
    let mut authority = vec!["a", "b"];
    let edit = remove_selected(
        &mut authority,
        &["b", "a", "ctx"],
        2,
        &[(0, "a"), (2, "ctx")],
    );
    assert!(edit.is_empty());
    assert_eq!(authority, ["a", "b"]);
}

#[test]
fn confirmation_fences_partial_reads_and_keeps_the_six_retry_fallback() {
    let mut router = Router::new(SessionId(10));
    let (ticket, _) = router.begin(Lane::ManualEdit).unwrap();
    let confirmation = Confirmation::new(ticket, 6_000, 6);
    let authority = ["a", "b", "b"];
    for retries in 0..6 {
        assert_eq!(
            confirmation.decide(&router, 1, retries, authority, ["a"]),
            D::Retry
        );
    }
    assert_eq!(
        confirmation.decide(&router, 1, 6, authority, ["a"]),
        D::Fallback
    );
    assert_eq!(
        confirmation.decide(&router, 6_000, 0, authority, ["a"]),
        D::Fallback
    );
    assert_eq!(
        confirmation.decide(&router, 1, 0, authority, authority),
        D::Confirmed
    );
    router.begin(Lane::ManualEdit).unwrap();
    assert_eq!(
        confirmation.decide(&router, 1, 0, authority, authority),
        D::Obsolete
    );
    let (fresh, _) = router.begin(Lane::ManualEdit).unwrap();
    let confirmation = Confirmation::new(fresh, 6_000, 6);
    router.replace_session(SessionId(11)).unwrap();
    assert_eq!(
        confirmation.decide(&router, 1, 0, authority, authority),
        D::Obsolete
    );
}
