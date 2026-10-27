// Included inside app::tests so the characterization uses the real private
// transitions and existing offline fixtures, not a second application model.
use super::*;
use doremi_core::queue::{
    Command as CoreCommand, Effect as CoreEffect, Event as CoreEvent, Repeat,
};

mod traces {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/doremi-core/tests/support/traces.rs"
    ));
}

fn uri(id: &str) -> String {
    format!("spotify:track:{id}")
}

fn listener() -> App {
    let mut app = headless_app();
    app.backend.set_offline(true);
    app.auth = AuthStatus::Connected {
        username: "fixture".into(),
    };
    app.local_ready = true;
    app.local.track = Some(crate::player::LocalTrack {
        uri: uri("a"),
        duration_ms: 60_000,
        ..Default::default()
    });
    app.local.playback = Playback::Playing;
    app.queue = loaded_queue(&uri("a"), &[&uri("c"), &uri("d")]);
    app
}

fn observe(app: &App) -> traces::Observation {
    let (current, next) = queue_uris(app);
    let short = |uri: String| uri.strip_prefix("spotify:track:").unwrap().to_owned();
    traces::Observation {
        current: current.map(short),
        manual: app.manual_queue.iter().cloned().map(short).collect(),
        upcoming: next.into_iter().map(short).collect(),
    }
}

fn confirm_queue(app: &mut App, current: &str, context: &[&str]) {
    // A provider confirmation, independent of the core's expected observation.
    // The engine report is represented by its existing local state snapshot.
    app.local.track = Some(crate::player::LocalTrack {
        uri: uri(current),
        duration_ms: 60_000,
        ..Default::default()
    });
    app.intent_track = None;
    let rows: Vec<String> = app
        .manual_queue
        .iter()
        .cloned()
        .chain(context.iter().map(|id| uri(id)))
        .collect();
    app.refresh_queue(true);
    app.handle_api(ApiResponse::Queue {
        seq: app.queue_seq,
        result: Ok(Queue {
            currently_playing: Some(queued_song(&uri(current))),
            queue: rows.iter().map(|id| queued_song(id)).collect(),
        }),
    });
}

#[test]
fn preserved_traces_match_core_and_legacy_after_every_transition() {
    let ctx = egui::Context::default();
    for trace in traces::traces() {
        let mut app = listener();
        let mut core = traces::initial();
        for (step, expected) in trace {
            match &step {
                traces::Step::Append(ids) => app.apply(
                    Action::QueueMany {
                        songs: ids.iter().map(|id| (uri(id), (*id).into())).collect(),
                    },
                    &ctx,
                ),
                traces::Step::Insert(position, ids) => app.apply(
                    Action::InsertInQueue {
                        position: *position,
                        items: ids.iter().map(|id| queued_song(&uri(id))).collect(),
                    },
                    &ctx,
                ),
                traces::Step::Move(from, to) => app.apply(
                    Action::MoveInQueue {
                        from: *from,
                        to: *to,
                    },
                    &ctx,
                ),
                traces::Step::Remove(index) => app.apply(
                    Action::RemoveFromQueue {
                        rows: vec![(*index, app.manual_queue[*index].clone())],
                    },
                    &ctx,
                ),
                traces::Step::Clear => app.clear_queue(),
                traces::Step::Next => {
                    app.apply(Action::Next, &ctx);
                    // Acknowledge this optimistic skip before the next command.
                    if let Some(current) =
                        app.queue.get().and_then(|q| q.currently_playing.as_ref())
                    {
                        app.local.track = Some(crate::player::LocalTrack {
                            uri: current.uri().into(),
                            ..Default::default()
                        });
                        app.intent_track = None;
                    }
                }
                traces::Step::Order(order) => {
                    app.apply(Action::SetShuffle(order.is_some()), &ctx);
                    app.local.shuffle = order.is_some();
                    let context = if order.is_some() {
                        &["d", "c"]
                    } else {
                        &["c", "d"]
                    };
                    confirm_queue(&mut app, "a", context);
                }
                traces::Step::Context(ids) => {
                    app.play_request(
                        PlayRequest::tracks(ids.iter().map(|id| uri(id)).collect())
                            .starting_at_index(0),
                        false,
                    );
                    confirm_queue(&mut app, ids[0], &ids[1..]);
                }
            }
            traces::apply(&mut core, &step);
            assert_eq!(observe(&app), expected, "legacy {step:?}");
            assert_eq!(traces::observe(&core), expected, "core {step:?}");
        }
        app.backend.shutdown();
    }
}

#[test]
fn frozen_prefix_rule_matches_bridge_for_duplicate_and_displaced_rows() {
    fn old_rule(rows: &[String], manual: &[String]) -> usize {
        let mut counts = std::collections::HashMap::new();
        for id in manual {
            *counts.entry(id.as_str()).or_insert(0_usize) += 1;
        }
        let mut at = 0;
        for id in rows {
            match counts.get_mut(id.as_str()) {
                Some(left) if *left > 0 => {
                    *left -= 1;
                    at += 1;
                }
                _ => break,
            }
        }
        at
    }
    fn lists(depth: usize) -> Vec<Vec<String>> {
        let mut result = vec![vec![]];
        let mut layer = vec![vec![]];
        for _ in 0..depth {
            layer = layer
                .iter()
                .flat_map(|list| {
                    ["a", "b"].map(|id| {
                        let mut next = list.clone();
                        next.push(uri(id));
                        next
                    })
                })
                .collect();
            result.extend(layer.clone());
        }
        result
    }
    for rows in lists(4) {
        for manual in lists(3) {
            let items: Vec<_> = rows.iter().map(|id| queued_song(id)).collect();
            assert_eq!(
                App::end_of_queued_rows(&items, &manual),
                old_rule(&rows, &manual)
            );
        }
    }
}

#[test]
fn clear_cancels_album_and_late_results_do_not_return_in_either_domain() {
    let mut app = listener();
    let mut core = traces::initial();
    app.queue_album("spotify:album:late".into(), "Late album".into());
    let request = app.album_queue_serial;
    let core_request = match core.command(CoreCommand::BeginAppend).unwrap()[0] {
        CoreEffect::ReadAppend(request) => request,
        _ => panic!("request"),
    };
    app.clear_queue();
    core.command(CoreCommand::ClearManual).unwrap();
    app.handle_api(ApiResponse::AlbumQueueTracks {
        request,
        offset: 0,
        result: Ok(crate::api::models::Page {
            items: vec![album_queue_track("late")],
            offset: 0,
            limit: 1,
            total: 1,
            next: None,
        }),
    });
    core.event(CoreEvent::Appended {
        request: core_request,
        media: vec![traces::media("late")],
    })
    .unwrap();
    assert_eq!(observe(&app), traces::observe(&core));
    assert!(app.pending_album_queues.is_empty());
    app.backend.shutdown();
}

#[test]
fn repeat_one_hand_change_and_restart_threshold_match_current_legacy_not_old_prose() {
    let ctx = egui::Context::default();
    for position in [3_000, 3_001] {
        let mut app = listener();
        app.local.position_ms = position;
        app.local.playback = Playback::Paused;
        app.local.repeat = RepeatMode::Track;
        app.apply(Action::Previous, &ctx);
        let mut core = traces::initial();
        core.command(CoreCommand::SetRepeat(Repeat::Track)).unwrap();
        core.command(CoreCommand::Seek(position.into())).unwrap();
        core.command(CoreCommand::Previous).unwrap();
        let expected = if position > 3_000 {
            Repeat::Track
        } else {
            Repeat::Context
        };
        assert_eq!(core.repeat(), expected);
        assert_eq!(
            app.local.repeat,
            if expected == Repeat::Track {
                RepeatMode::Track
            } else {
                RepeatMode::Context
            }
        );
        assert!(
            app.backend
                .take_player_commands()
                .iter()
                .any(|c| matches!(c, PlayerCommand::Previous))
        );
        app.backend.shutdown();
    }
}

#[test]
fn old_poll_cannot_undo_the_shared_reorder_rule() {
    let ctx = egui::Context::default();
    let mut app = listener();
    app.manual_queue = vec![uri("b"), uri("e")];
    app.queue = loaded_queue(&uri("a"), &[&uri("b"), &uri("e"), &uri("c"), &uri("d")]);
    app.refresh_queue(true);
    let old_seq = app.queue_seq;
    let old = app.queue.get().unwrap().clone();
    app.apply(Action::MoveInQueue { from: 0, to: 2 }, &ctx);
    let mut core = traces::initial();
    core.command(CoreCommand::Append(vec![
        traces::media("b"),
        traces::media("e"),
    ]))
    .unwrap();
    let entry = core.manual()[0].id;
    core.command(CoreCommand::Move { entry, gap: 2 }).unwrap();
    app.handle_api(ApiResponse::Queue {
        seq: old_seq,
        result: Ok(old),
    });
    assert_eq!(observe(&app), traces::observe(&core));
    app.backend.shutdown();
}

#[test]
fn remembered_previous_restart_and_manual_restore_are_paused() {
    let ctx = egui::Context::default();
    let mut app = headless_app();
    app.resume_track = Some(uri("a"));
    app.resume_position_ms = 3_001;
    app.resume_queue = vec![uri("b"), uri("b")];
    app.apply(Action::Previous, &ctx);
    assert_eq!(app.resume_position_ms, 0);
    assert_eq!(app.resume_queue, [uri("b"), uri("b")]);
    assert!(app.resume_only());
    assert!(app.queued_play.is_none());
    let mut core = traces::initial();
    core.command(CoreCommand::Append(vec![
        traces::media("b"),
        traces::media("b"),
    ]))
    .unwrap();
    core.command(CoreCommand::Seek(3_001)).unwrap();
    let saved = core.snapshot();
    core.command(CoreCommand::Restore(saved.into())).unwrap();
    core.command(CoreCommand::Previous).unwrap();
    assert_eq!(core.position_ms(), u64::from(app.resume_position_ms));
    assert_eq!(core.current().unwrap().media.id.local(), "a");
    assert_eq!(
        core.manual()
            .iter()
            .map(|e| uri(e.media.id.local()))
            .collect::<Vec<_>>(),
        app.resume_queue
    );
    assert_eq!(core.playback(), doremi_core::queue::Playback::Paused);
    // No provider work is started by browsing a paused restored bookmark.
    app.backend.shutdown();
}
