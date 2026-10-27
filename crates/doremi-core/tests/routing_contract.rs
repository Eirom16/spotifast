use doremi_core::queue::{Command, Effect, Event, Order, Playback, Queue};
use doremi_core::requests::{Lane, Router, RoutingError, SessionId};

use doremi_core::media::{Media, MediaId, ProviderId};

#[test]
fn independent_lanes_cancel_supersede_and_reject_cross_session_replies() {
    let mut router = Router::new(SessionId(7));
    let (search, _) = router.begin(Lane::Search).unwrap();
    let (resolve, _) = router.begin(Lane::Resolve).unwrap();
    let (new_search, cancelled) = router.begin(Lane::Search).unwrap();
    assert_eq!(cancelled, Some(search));
    assert!(!router.accepts(search));
    assert!(router.accepts(resolve));
    assert!(router.finish(new_search));
    assert!(!router.finish(new_search));
    assert_eq!(router.cancel(Lane::Resolve), Some(resolve));
    let (old, _) = router.begin(Lane::ManualEdit).unwrap();
    assert_eq!(
        router.replace_session(SessionId(7)),
        Err(RoutingError::SessionNotNewer)
    );
    assert_eq!(router.replace_session(SessionId(8)).unwrap(), [old]);
    assert!(!router.accepts(old));
}

#[test]
fn fresh_queue_restoration_rejects_even_old_callbacks_created_after_the_checkpoint() {
    fn ticket(effects: Vec<Effect>) -> doremi_core::queue::RequestId {
        effects
            .into_iter()
            .find_map(|e| match e {
                Effect::Load { request, .. } => Some(request),
                _ => None,
            })
            .unwrap()
    }
    let mut old = Queue::new(SessionId(1));
    old.command(Command::StartContext {
        media: vec![Media {
            id: MediaId::new(ProviderId::new("fixture").unwrap(), "a").unwrap(),
            title: "a".into(),
            duration_ms: None,
        }],
        start: 0,
        order: Order::Sequential,
    })
    .unwrap();
    let saved = old.snapshot();
    let late = ticket(old.command(Command::Play).unwrap());
    let mut restored = Queue::new(SessionId(2));
    restored.command(Command::Restore(saved.into())).unwrap();
    let fresh = ticket(restored.command(Command::Play).unwrap());
    restored.event(Event::Ready(late)).unwrap();
    assert_eq!(restored.playback(), Playback::Loading);
    restored.event(Event::Ready(fresh)).unwrap();
    assert_eq!(restored.playback(), Playback::Playing);
    restored.replace_session(SessionId(3)).unwrap();
    restored.event(Event::Ended(fresh)).unwrap();
    assert_eq!(restored.playback(), Playback::Paused);
    assert_eq!(restored.current().unwrap().media.id.local(), "a");
}
