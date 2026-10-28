use super::*;

fn uri(id: &str) -> String {
    format!("spotify:track:{id}")
}

fn local() -> App {
    let mut app = app_with_local_manual_queue();
    app.backend.set_offline(true);
    app
}

#[test]
fn duplicate_removal_edits_the_only_authority_and_resyncs_spirc_in_that_order() {
    let mut app = local();
    app.manual_queue = ["b", "x", "b", "y"].map(uri).to_vec();
    app.queue = loaded_queue(
        "spotify:track:playing",
        &[
            &uri("x"),
            &uri("b"),
            &uri("b"),
            &uri("y"),
            &uri("ctx"),
            &uri("b"),
        ],
    );
    app.pending_queue_adds = app
        .manual_queue
        .iter()
        .enumerate()
        .map(|(index, key)| PendingQueueAdd {
            item: queued_song(key),
            at: Instant::now(),
            manual_index: index,
            write: Some((17, index)),
        })
        .collect();
    app.backend.take_player_commands();
    app.apply(
        Action::RemoveFromQueue {
            rows: vec![(2, uri("b")), (2, uri("b")), (5, uri("b"))],
        },
        &egui::Context::default(),
    );
    assert_eq!(app.manual_queue, ["b", "x", "y"].map(uri));
    assert_eq!(queue_uris(&app).1, ["x", "b", "y", "ctx", "b"].map(uri));
    assert_eq!(app.pending_queue_adds.len(), 3);
    for addition in &app.pending_queue_adds {
        assert_eq!(app.manual_queue[addition.manual_index], addition.item.uri());
    }
    assert_eq!(app.pending_queue_adds[2].write, Some((17, 3)));
    let commands = app.backend.take_player_commands();
    assert!(matches!(commands.first(), Some(PlayerCommand::ClearQueue)));
    assert_eq!(commands.len(), 4);
    for (command, key) in commands.iter().skip(1).zip(&app.manual_queue) {
        assert!(matches!(command, PlayerCommand::AddToQueue(uri) if uri == key));
    }
    app.backend.shutdown();
}

#[test]
fn partial_confirmation_retries_then_consumes_the_matching_reply_once() {
    let mut app = local();
    app.refresh_queue(true);
    let old_seq = app.queue_seq;
    let old = app.queue.get().unwrap().clone();
    app.apply(
        Action::RemoveFromQueue {
            rows: vec![(1, uri("b"))],
        },
        &egui::Context::default(),
    );
    let expected = app.manual_queue.clone();
    app.handle_api(ApiResponse::Queue {
        seq: old_seq,
        result: Ok(old),
    });
    assert_eq!(app.manual_queue, expected);
    app.refresh_queue(true);
    let seq = app.queue_seq;
    app.handle_api(ApiResponse::Queue {
        seq,
        result: Ok(Queue {
            currently_playing: Some(queued_song("spotify:track:playing")),
            queue: vec![queued_song(&uri("a"))],
        }),
    });
    assert_eq!(app.queue_stale_retries, 1);
    assert!(app.queue_reorder_pending.is_some());
    assert_eq!(app.manual_queue, expected);
    let confirmed = Queue {
        currently_playing: Some(queued_song("spotify:track:playing")),
        queue: ["a", "c", "ctx"]
            .iter()
            .map(|id| queued_song(&uri(id)))
            .collect(),
    };
    app.handle_api(ApiResponse::Queue {
        seq,
        result: Ok(confirmed),
    });
    assert!(app.queue_reorder_pending.is_none());
    assert_eq!(app.queue_stale_retries, 0);
    let view = queue_uris(&app);
    app.handle_api(ApiResponse::Queue {
        seq,
        result: Ok(Queue::default()),
    });
    assert_eq!(queue_uris(&app), view);
    app.backend.shutdown();
}

#[test]
fn partial_writes_and_read_errors_never_create_a_second_manual_authority() {
    let mut app = local();
    let pending_start = app.pending_queue_adds.len();
    app.show_queued_song(&uri("x"), "pending fixture");
    app.write_queue_adds(vec![uri("x")], pending_start);
    let write = app.album_queue_serial;
    app.apply(
        Action::RemoveFromQueue {
            rows: vec![(1, uri("b"))],
        },
        &egui::Context::default(),
    );
    assert_eq!(app.manual_queue, ["a", "c", "x"].map(uri));
    app.refresh_queue(true);
    app.handle_api(ApiResponse::Queue {
        seq: app.queue_seq,
        result: Ok(Queue {
            currently_playing: Some(queued_song("spotify:track:playing")),
            queue: vec![],
        }),
    });
    assert_eq!(app.manual_queue, ["a", "c", "x"].map(uri));
    assert!(app.queue_reorder_pending.is_some());
    // Only the failed write is rolled back, not the manual edit or its siblings.
    app.handle_api(ApiResponse::QueueBatchAdded {
        request: write,
        added: 0,
        result: Err(crate::api::ApiError::QuotaExhausted),
    });
    assert_eq!(app.manual_queue, ["a", "c"].map(uri));
    app.handle_api(ApiResponse::Queue {
        seq: app.queue_seq,
        result: Err(crate::api::ApiError::QuotaExhausted),
    });
    assert!(app.queue_reorder_pending.is_some());
    assert_eq!(app.manual_queue, ["a", "c"].map(uri));
    app.refresh_queue(true);
    app.handle_api(ApiResponse::Queue {
        seq: app.queue_seq,
        result: Ok(Queue {
            currently_playing: Some(queued_song("spotify:track:playing")),
            queue: ["a", "c", "ctx"]
                .iter()
                .map(|id| queued_song(&uri(id)))
                .collect(),
        }),
    });
    assert!(app.queue_reorder_pending.is_none());
    assert_eq!(app.manual_queue, ["a", "c"].map(uri));
    app.backend.shutdown();
}

#[test]
fn sign_out_replacement_and_restoration_cannot_accept_a_previous_sessions_poll() {
    let mut app = local();
    app.refresh_queue(true);
    let old_seq = app.queue_seq;
    let mut old = app.queue.get().unwrap().clone();
    old.queue = vec![queued_song(&uri("old-account"))];
    let session = app.queue_edit_router.session();
    app.reset_data();
    assert!(app.queue_edit_router.session() > session);
    // Restore just the non-secret displayed rows, not transport/session state.
    app.queue = loaded_queue(
        "spotify:track:playing",
        &[&uri("a"), &uri("b"), &uri("c"), &uri("ctx")],
    );
    app.handle_api(ApiResponse::Queue {
        seq: old_seq,
        result: Ok(old),
    });
    assert_eq!(queue_uris(&app).1, ["a", "b", "c", "ctx"].map(uri));
    assert!(app.queue_reorder_pending.is_none());
    app.backend.shutdown();
}

#[test]
fn stale_occurrence_and_resume_only_edits_do_not_mutate_a_different_row() {
    let mut app = local();
    let before = app.manual_queue.clone();
    app.apply(
        Action::RemoveFromQueue {
            rows: vec![(0, uri("b"))],
        },
        &egui::Context::default(),
    );
    assert_eq!(app.manual_queue, before);
    app.local = LocalState::default();
    app.resume_track = Some(uri("a"));
    app.resume_queue = app.manual_queue.clone();
    app.apply(
        Action::RemoveFromQueue {
            rows: vec![(0, uri("a"))],
        },
        &egui::Context::default(),
    );
    assert_eq!(app.manual_queue, before);
    assert_eq!(app.resume_queue, before);
    app.backend.shutdown();
}

#[test]
fn resume_only_wrap_and_duplicate_uri_ambiguity_are_characterized_not_replaced() {
    let mut app = headless_app();
    app.backend.set_offline(true);
    app.playlist_pages.insert(
        "fixture".into(),
        PlaylistPage {
            items: PagedList {
                items: ["a", "b", "b", "c"]
                    .iter()
                    .map(|id| crate::api::models::PlaylistItem {
                        item: Some(queued_song(&uri(id))),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    app.resume_context = Some("spotify:playlist:fixture".into());
    app.resume_track = Some(uri("a"));
    let ctx = egui::Context::default();
    app.apply(Action::Previous, &ctx);
    assert_eq!(app.resume_track, Some(uri("c")));
    app.apply(Action::Next, &ctx);
    assert_eq!(app.resume_track, Some(uri("a")));
    app.apply(Action::Next, &ctx);
    assert_eq!(app.resume_track, Some(uri("b")));
    app.apply(Action::Next, &ctx);
    assert_eq!(app.resume_track, Some(uri("b")));
    app.apply(Action::Next, &ctx);
    assert_eq!(
        app.resume_track,
        Some(uri("b")),
        "legacy bookmark locates the first URI, not a global occurrence"
    );
    assert!(app.queued_play.is_none());
    app.backend.shutdown();
}
