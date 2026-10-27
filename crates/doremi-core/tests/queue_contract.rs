#[path = "support/traces.rs"]
mod traces;

use doremi_core::media::{IdentityError, MediaId, ProviderId};
use doremi_core::queue::{
    Command, Effect, Event, MovePlan, Order, Playback, Queue, QueueError, Repeat, manual_prefix_len,
};
use traces::{initial, media};

fn load(effects: Vec<Effect>) -> doremi_core::queue::RequestId {
    effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::Load { request, .. } => Some(request),
            _ => None,
        })
        .expect("load request")
}

fn append_request(queue: &mut Queue) -> doremi_core::queue::RequestId {
    match queue.command(Command::BeginAppend).unwrap().last().unwrap() {
        Effect::ReadAppend(request) => *request,
        _ => panic!("append request"),
    }
}

#[test]
fn shared_legacy_traces_replay_step_by_step() {
    for trace in traces::traces() {
        let mut queue = initial();
        for (step, expected) in trace {
            traces::apply(&mut queue, &step);
            assert_eq!(traces::observe(&queue), expected, "{step:?}");
        }
    }
}

#[test]
fn provider_namespaces_and_occurrences_are_not_song_titles() {
    assert_eq!(
        ProviderId::new("Not a namespace"),
        Err(IdentityError::InvalidProvider)
    );
    let a = MediaId::new(ProviderId::new("one").unwrap(), "same").unwrap();
    let b = MediaId::new(ProviderId::new("two").unwrap(), "same").unwrap();
    assert_ne!(a, b);
    assert_eq!(
        MediaId::new(ProviderId::new("one").unwrap(), ""),
        Err(IdentityError::EmptyLocalId)
    );
    let mut q = initial();
    q.command(Command::Append(vec![media("b"), media("b")]))
        .unwrap();
    assert_ne!(q.manual()[0].id, q.manual()[1].id);
    assert_eq!(q.manual()[0].media, q.manual()[1].media);
}

#[test]
fn every_drag_plan_matches_remove_insert_and_maps_pending_occurrences() {
    for len in 0..9 {
        for from in 0..=len {
            for gap in 0..=len + 1 {
                let Some(plan) = MovePlan::new(len, from, gap) else {
                    continue;
                };
                let mut rows: Vec<_> = (0..len).collect();
                plan.apply(&mut rows);
                let mut expected: Vec<_> = (0..len).collect();
                let item = expected.remove(from);
                expected.insert(if gap > from { gap - 1 } else { gap }, item);
                assert_eq!(rows, expected);
                for old in 0..len {
                    assert_eq!(rows[plan.remap(old)], old);
                }
            }
        }
    }
}

#[test]
fn manual_prefix_is_contiguous_and_counts_duplicates_once_per_occurrence() {
    assert_eq!(manual_prefix_len(["b", "b", "b", "c"], ["b", "b"]), 2);
    assert_eq!(manual_prefix_len(["b", "c", "b"], ["b", "b"]), 1);
    assert_eq!(manual_prefix_len(["c", "b"], ["b"]), 0);
    assert_eq!(manual_prefix_len(["b", "a", "b"], ["a", "b", "b"]), 3);
}

#[test]
fn invalid_edits_and_permutations_are_transactional() {
    let mut q = initial();
    q.command(Command::Append(vec![media("b")])).unwrap();
    for command in [
        Command::Insert {
            gap: 2,
            media: vec![media("bad")],
        },
        Command::SetOrder(Order::Shuffled(vec![0, 0, 2])),
        Command::SetOrder(Order::Shuffled(vec![0, 1, 3])),
        Command::SetOrder(Order::Shuffled(vec![0])),
        Command::StartContext {
            media: vec![],
            start: 0,
            order: Order::Sequential,
        },
        Command::Move {
            entry: q.current().unwrap().id,
            gap: 0,
        },
        Command::Remove(q.current().unwrap().id),
    ] {
        let before = q.clone();
        assert!(q.command(command).is_err());
        assert_eq!(q, before);
    }
    let before = q.clone();
    assert_eq!(
        q.command(Command::Insert {
            gap: 9,
            media: vec![]
        }),
        Err(QueueError::InvalidIndex)
    );
    assert_eq!(q, before);
}

#[test]
fn late_load_position_error_and_end_cannot_undo_a_newer_track() {
    let mut q = initial();
    let old = load(q.command(Command::Play).unwrap());
    let current = load(q.command(Command::Next).unwrap());
    q.event(Event::Ready(current)).unwrap();
    let before = q.clone();
    for event in [
        Event::Ready(old),
        Event::Ended(old),
        Event::Failed(old),
        Event::Position {
            request: old,
            position_ms: 99,
        },
    ] {
        assert!(q.event(event).unwrap().is_empty());
        assert_eq!(q, before);
    }
    assert_eq!(q.playback(), Playback::Playing);
}

#[test]
fn pause_during_load_is_preserved_and_seek_supersedes_ready() {
    let mut q = initial();
    let old = load(q.command(Command::Play).unwrap());
    q.command(Command::Pause).unwrap();
    assert_eq!(
        q.event(Event::Ready(old)).unwrap(),
        [Effect::SetPlaying(false)]
    );
    assert_eq!(q.playback(), Playback::Paused);
    let seek = load(q.command(Command::Seek(200_000)).unwrap());
    q.event(Event::Ready(old)).unwrap();
    assert_eq!(q.playback(), Playback::Loading);
    q.event(Event::Ready(seek)).unwrap();
    assert_eq!(q.position_ms(), 60_000);
    assert_eq!(q.playback(), Playback::Paused);
}

#[test]
fn clear_and_context_change_cancel_only_pending_appends_not_current_audio() {
    for clear in [true, false] {
        let mut q = initial();
        let request = append_request(&mut q);
        if clear {
            let effects = q.command(Command::ClearManual).unwrap();
            assert_eq!(effects, vec![Effect::Cancel(request)]);
            assert_eq!(q.current().unwrap().media.id.local(), "a");
        } else {
            q.command(Command::StartContext {
                media: vec![media("x")],
                start: 0,
                order: Order::Sequential,
            })
            .unwrap();
        }
        let before = q.clone();
        q.event(Event::Appended {
            request,
            media: vec![media("late")],
        })
        .unwrap();
        assert_eq!(q, before);
    }
}

#[test]
fn append_failure_and_supersession_do_not_commit_partial_albums() {
    let mut q = initial();
    let old = append_request(&mut q);
    let fresh = append_request(&mut q);
    q.event(Event::Appended {
        request: old,
        media: vec![media("stale")],
    })
    .unwrap();
    assert!(q.manual().is_empty());
    q.event(Event::AppendFailed(fresh)).unwrap();
    q.event(Event::Appended {
        request: fresh,
        media: vec![media("late")],
    })
    .unwrap();
    assert!(q.manual().is_empty());
    let fresh = append_request(&mut q);
    q.event(Event::Appended {
        request: fresh,
        media: vec![media("b"), media("b")],
    })
    .unwrap();
    assert_eq!(q.manual().len(), 2);
}

#[test]
fn cancel_stops_audio_but_late_ready_cannot_resume_it() {
    let mut q = initial();
    let request = load(q.command(Command::Play).unwrap());
    q.command(Command::Cancel).unwrap();
    let before = q.clone();
    q.event(Event::Ready(request)).unwrap();
    assert_eq!(q, before);
    assert_eq!(q.playback(), Playback::Paused);
}

#[test]
fn repeat_track_replays_on_end_but_explicit_next_advances() {
    let mut q = initial();
    q.command(Command::SetRepeat(Repeat::Track)).unwrap();
    let request = load(q.command(Command::Play).unwrap());
    q.event(Event::Ready(request)).unwrap();
    let replay = load(q.event(Event::Ended(request)).unwrap());
    assert_ne!(request, replay);
    assert_eq!(q.current().unwrap().media.id.local(), "a");
    q.command(Command::Next).unwrap();
    assert_eq!(q.current().unwrap().media.id.local(), "c");
    assert_eq!(q.repeat(), Repeat::Context);
}

#[test]
fn repeat_context_wraps_in_exact_order_and_manual_songs_do_not_repeat() {
    let mut q = initial();
    q.command(Command::SetRepeat(Repeat::Context)).unwrap();
    q.command(Command::SetOrder(Order::Shuffled(vec![0, 2, 1])))
        .unwrap();
    q.command(Command::Append(vec![media("b")])).unwrap();
    let mut heard = Vec::new();
    for _ in 0..5 {
        q.command(Command::Next).unwrap();
        heard.push(q.current().unwrap().media.id.local().to_owned());
    }
    assert_eq!(heard, ["b", "d", "c", "a", "d"]);
    assert!(q.manual().is_empty());
}

#[test]
fn previous_restarts_strictly_after_three_seconds_else_replays_heard_history() {
    let mut q = initial();
    let current = load(q.command(Command::Next).unwrap());
    q.event(Event::Ready(current)).unwrap();
    q.event(Event::Position {
        request: current,
        position_ms: 3_001,
    })
    .unwrap();
    let restarted = load(q.command(Command::Previous).unwrap());
    assert_eq!(q.current().unwrap().media.id.local(), "c");
    q.event(Event::Ready(restarted)).unwrap();
    q.event(Event::Position {
        request: restarted,
        position_ms: 3_000,
    })
    .unwrap();
    q.command(Command::Previous).unwrap();
    assert_eq!(q.current().unwrap().media.id.local(), "a");
    assert_eq!(
        q.upcoming().map(|e| e.media.id.local()).collect::<Vec<_>>(),
        ["c", "d"]
    );
    q.command(Command::Next).unwrap();
    assert_eq!(q.current().unwrap().media.id.local(), "c");
}

#[test]
fn restoration_keeps_duplicates_order_repeat_and_position_but_not_live_requests() {
    let mut q = initial();
    q.command(Command::Append(vec![media("b"), media("b")]))
        .unwrap();
    q.command(Command::SetOrder(Order::Shuffled(vec![0, 2, 1])))
        .unwrap();
    q.command(Command::SetRepeat(Repeat::Context)).unwrap();
    let old = load(q.command(Command::Play).unwrap());
    q.event(Event::Ready(old)).unwrap();
    q.event(Event::Position {
        request: old,
        position_ms: 42_000,
    })
    .unwrap();
    let saved = q.snapshot();
    let mut restored = Queue::default();
    restored.command(Command::Restore(saved.into())).unwrap();
    assert_eq!(traces::observe(&restored), traces::observe(&q));
    assert_eq!(restored.repeat(), Repeat::Context);
    assert!(restored.shuffled());
    assert_eq!(restored.position_ms(), 42_000);
    assert_eq!(restored.playback(), Playback::Paused);
    let fresh = load(restored.command(Command::Play).unwrap());
    assert_ne!(fresh, old);
    let before = restored.clone();
    restored.event(Event::Ready(old)).unwrap();
    assert_eq!(restored, before);
    restored.event(Event::Ready(fresh)).unwrap();
    assert_eq!(restored.playback(), Playback::Playing);
}

#[test]
fn end_without_repeat_stops_and_error_cannot_become_a_late_success() {
    let mut q = initial();
    q.command(Command::Next).unwrap();
    let request = load(q.command(Command::Next).unwrap());
    q.event(Event::Ready(request)).unwrap();
    q.event(Event::Ended(request)).unwrap();
    assert_eq!(q.playback(), Playback::Stopped);
    assert!(q.current().is_none());
    let mut q = initial();
    let request = load(q.command(Command::Play).unwrap());
    assert_eq!(q.event(Event::Failed(request)).unwrap(), [Effect::Stop]);
    q.event(Event::Ready(request)).unwrap();
    assert_eq!(q.playback(), Playback::Failed);
}
