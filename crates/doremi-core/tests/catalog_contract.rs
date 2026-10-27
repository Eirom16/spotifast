use doremi_core::catalog::*;
use doremi_core::media::{Media, MediaId, ProviderId};
use doremi_core::requests::{SessionId, Ticket};

fn provider() -> ProviderId {
    ProviderId::new("fixture").unwrap()
}
fn media(id: &str) -> Media {
    Media {
        id: MediaId::new(provider(), id).unwrap(),
        title: id.into(),
        duration_ms: None,
    }
}
fn consumer() -> Catalog {
    Catalog::new(
        SessionId(1),
        Capabilities {
            provider: provider(),
            search: true,
            resolve: true,
            max_results: 20,
            formats: vec![AudioFormat::WavPcm16],
        },
    )
    .unwrap()
}
fn request(c: &mut Catalog, now: Tick) -> Ticket {
    c.search("authorized fixture".into(), 2, now, 100)
        .unwrap()
        .iter()
        .find_map(|cmd| match cmd {
            ProviderCommand::Search(r) => Some(r.ticket),
            _ => None,
        })
        .unwrap()
}
fn result(c: &mut Catalog) {
    let ticket = request(c, Tick(0));
    c.complete(
        ProviderEvent::Search {
            ticket,
            result: Ok(vec![media("a")]),
        },
        Tick(1),
    )
    .unwrap();
}
fn resolution(c: &mut Catalog, now: Tick) -> Ticket {
    c.resolve(media("a").id, now, 100)
        .unwrap()
        .iter()
        .find_map(|cmd| match cmd {
            ProviderCommand::Resolve(r) => Some(r.ticket),
            _ => None,
        })
        .unwrap()
}
fn stream(format: AudioFormat, until: Tick) -> TemporaryResource {
    TemporaryResource::http(
        media("a").id,
        "https://fixture.invalid/audio?secret=signed".into(),
        vec![
            ("Authorization".into(), "private-grant".into()),
            ("Cookie".into(), "private-cookie".into()),
        ],
        StreamInfo {
            format,
            usable_until: until,
            lease_basis: LeaseBasis::ConservativeHostLimit,
            byte_len: None,
            seekable: true,
        },
    )
    .unwrap()
}

#[test]
fn fake_search_selection_and_resolution_use_ids_and_redact_sensitive_resources() {
    let mut c = consumer();
    result(&mut c);
    assert_eq!(c.search_status(), Status::Ready);
    let ticket = resolution(&mut c, Tick(2));
    let resource = stream(AudioFormat::WavPcm16, Tick(1000));
    let debug = format!("{resource:?}");
    for secret in [
        "signed",
        "private-grant",
        "private-cookie",
        "fixture.invalid",
    ] {
        assert!(!debug.contains(secret));
    }
    c.complete(
        ProviderEvent::Resolve {
            ticket,
            result: Ok(resource),
        },
        Tick(3),
    )
    .unwrap();
    assert_eq!(c.resolve_status(), Status::Ready);
    assert_eq!(c.resource().unwrap().expose_headers().count(), 2);
    assert_eq!(c.results()[0].id, media("a").id);
}

#[test]
fn invalid_queries_limits_capabilities_and_unknown_selection_are_rejected() {
    let mut c = consumer();
    for (query, limit, budget) in [
        ("".into(), 1, 1),
        ("x".into(), 0, 1),
        ("x".into(), 21, 1),
        ("x".repeat(513), 1, 1),
        ("x".into(), 1, 0),
    ] {
        assert_eq!(
            c.search(query, limit, Tick(0), budget).unwrap_err(),
            ProviderError::InvalidRequest
        );
    }
    assert_eq!(
        c.resolve(media("not-searched").id, Tick(0), 1).unwrap_err(),
        ProviderError::InvalidRequest
    );
    assert_eq!(c.search_status(), Status::Idle);
    let mut disabled = Catalog::new(
        SessionId(2),
        Capabilities {
            provider: provider(),
            search: false,
            resolve: false,
            max_results: 0,
            formats: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        disabled.search("x".into(), 1, Tick(0), 10).unwrap_err(),
        ProviderError::Unsupported
    );
    assert!(
        Catalog::new(
            SessionId(3),
            Capabilities {
                provider: provider(),
                search: true,
                resolve: true,
                max_results: MAX_RESULTS + 1,
                formats: vec![AudioFormat::WavPcm16],
            }
        )
        .is_err()
    );
}

#[test]
fn oversized_wrong_namespace_and_unbounded_metadata_are_not_committed() {
    for items in [
        vec![media("a"), media("b"), media("c")],
        vec![Media {
            id: MediaId::new(ProviderId::new("other").unwrap(), "a").unwrap(),
            ..media("a")
        }],
        vec![Media {
            title: "x".repeat(4097),
            ..media("a")
        }],
    ] {
        let mut c = consumer();
        let ticket = request(&mut c, Tick(0));
        c.complete(
            ProviderEvent::Search {
                ticket,
                result: Ok(items),
            },
            Tick(1),
        )
        .unwrap();
        assert!(matches!(c.search_status(), Status::Failed(_)));
        assert!(c.results().is_empty());
    }
}

#[test]
fn cancellation_supersession_and_session_replacement_fence_late_events() {
    let mut c = consumer();
    let old = request(&mut c, Tick(0));
    let fresh = request(&mut c, Tick(1));
    c.complete(
        ProviderEvent::Search {
            ticket: old,
            result: Ok(vec![media("old")]),
        },
        Tick(2),
    )
    .unwrap();
    assert_eq!(c.search_status(), Status::Pending);
    c.complete(
        ProviderEvent::Search {
            ticket: fresh,
            result: Ok(vec![media("a")]),
        },
        Tick(3),
    )
    .unwrap();
    let resolve = resolution(&mut c, Tick(4));
    assert!(!c.cancel().is_empty());
    c.complete(
        ProviderEvent::Resolve {
            ticket: resolve,
            result: Ok(stream(AudioFormat::WavPcm16, Tick(1000))),
        },
        Tick(5),
    )
    .unwrap();
    assert!(c.resource().is_none());
    let old = request(&mut c, Tick(6));
    c.replace_session(SessionId(2)).unwrap();
    let fresh = request(&mut c, Tick(7));
    assert_ne!(old, fresh);
    c.complete(
        ProviderEvent::Search {
            ticket: old,
            result: Ok(vec![media("late")]),
        },
        Tick(8),
    )
    .unwrap();
    assert_eq!(c.search_status(), Status::Pending);
}

#[test]
fn exact_deadline_wins_over_a_response_and_clock_regression_is_rejected() {
    let mut c = consumer();
    let ticket = request(&mut c, Tick(0));
    let cancelled = c
        .complete(
            ProviderEvent::Search {
                ticket,
                result: Ok(vec![media("late")]),
            },
            Tick(100),
        )
        .unwrap();
    assert!(matches!(cancelled.as_slice(), [ProviderCommand::Cancel(_)]));
    assert_eq!(c.search_status(), Status::Failed(ProviderError::Timeout));
    assert_eq!(
        c.advance(Tick(99)).unwrap_err(),
        ProviderError::InvalidRequest
    );
}

#[test]
fn temporary_resources_expire_and_never_become_library_metadata() {
    let mut c = consumer();
    result(&mut c);
    let ticket = resolution(&mut c, Tick(2));
    c.complete(
        ProviderEvent::Resolve {
            ticket,
            result: Ok(stream(AudioFormat::WavPcm16, Tick(10))),
        },
        Tick(3),
    )
    .unwrap();
    assert!(c.resource().is_some());
    c.advance(Tick(10)).unwrap();
    assert!(c.resource().is_none());
    assert_eq!(c.results(), [media("a")]);
}

#[test]
fn unsupported_format_expiry_and_wrong_media_cannot_publish_a_resource() {
    for resource in [
        stream(AudioFormat::OpusWebm, Tick(1000)),
        stream(AudioFormat::WavPcm16, Tick(2)),
        TemporaryResource::http(
            media("other").id,
            "https://fixture.invalid/a".into(),
            vec![],
            stream(AudioFormat::WavPcm16, Tick(1000)).info(),
        )
        .unwrap(),
    ] {
        let mut c = consumer();
        result(&mut c);
        let ticket = resolution(&mut c, Tick(2));
        c.complete(
            ProviderEvent::Resolve {
                ticket,
                result: Ok(resource),
            },
            Tick(3),
        )
        .unwrap();
        assert!(matches!(c.resolve_status(), Status::Failed(_)));
        assert!(c.resource().is_none());
    }
}

#[test]
fn typed_failures_remain_failures_and_are_not_silent_fallbacks() {
    for error in [
        ProviderError::AccessDenied,
        ProviderError::RateLimited {
            retry_after_ms: Some(50),
        },
        ProviderError::Transport,
        ProviderError::Unavailable,
        ProviderError::Cancelled,
    ] {
        let mut c = consumer();
        let ticket = request(&mut c, Tick(0));
        c.complete(
            ProviderEvent::Search {
                ticket,
                result: Err(error),
            },
            Tick(1),
        )
        .unwrap();
        assert_eq!(c.search_status(), Status::Failed(error));
        assert!(c.results().is_empty());
    }
}

#[test]
fn malformed_resource_metadata_is_rejected_without_logging_payloads() {
    let info = stream(AudioFormat::WavPcm16, Tick(100)).info();
    for uri in [
        "file:///private",
        "https://user:password@host/a",
        "https://",
        "https://x/a\r\ninjected",
    ] {
        assert_eq!(
            TemporaryResource::http(media("a").id, uri.into(), vec![], info).unwrap_err(),
            ProviderError::InvalidResponse
        );
    }
    assert!(
        TemporaryResource::http(
            media("a").id,
            "https://x/a".into(),
            vec![("Cookie".into(), "a\r\nb".into())],
            info
        )
        .is_err()
    );
}
