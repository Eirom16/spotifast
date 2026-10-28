use doremi_core::{
    media::{Media, MediaId, ProviderId},
    queue::{Command, Effect, Event, Order, Playback, Queue, Repeat},
    requests::SessionId,
};

fn media() -> Vec<Media> {
    (0..3)
        .map(|i| Media {
            id: MediaId::new(ProviderId::new("test").unwrap(), i.to_string()).unwrap(),
            title: format!("song {i}"),
            duration_ms: Some(10_000),
        })
        .collect()
}
fn ready(q: &mut Queue, command: Command) {
    if let Ok(effects) = q.command(command) {
        for effect in effects {
            if let Effect::Load { request, .. } = effect {
                q.event(Event::Ready(request)).unwrap();
            }
        }
    }
}
#[test]
fn reachable_queues_round_trip_occurrences_history_order_and_position_paused() {
    let mut q = Queue::new(SessionId(1));
    let mut seed = 73_u64;
    for step in 0..2000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let command = match seed % 14 {
            0 => Command::StartContext {
                media: media(),
                start: (seed as usize / 14) % 3,
                order: Order::Sequential,
            },
            1 | 2 => Command::Append(vec![media()[1].clone()]),
            3 => Command::Next,
            4 => Command::Previous,
            5 => Command::Pause,
            6 => Command::Play,
            7 => Command::Seek((seed / 14) % 11_000),
            8 => Command::SetRepeat(Repeat::Track),
            9 => Command::SetOrder(Order::Shuffled((0..q.context().len()).rev().collect())),
            10 => Command::SetOrder(Order::Sequential),
            11 => Command::ClearManual,
            12 if !q.manual().is_empty() => Command::Remove(q.manual()[0].id),
            _ => Command::Cancel,
        };
        ready(&mut q, command);
        let mut checkpoint = q.checkpoint();
        checkpoint.validate().unwrap();
        let mut restored = Queue::new(SessionId(step + 2));
        restored.restore_checkpoint(checkpoint.clone()).unwrap();
        let observed = restored.checkpoint();
        checkpoint.request_watermark = observed.request_watermark;
        assert_eq!(observed, checkpoint, "step {step}");
        assert_eq!(
            restored.playback(),
            if restored.current().is_some() {
                Playback::Paused
            } else {
                Playback::Stopped
            }
        );
    }
}
#[test]
fn invalid_checkpoint_cannot_replace_a_live_queue() {
    let mut q = Queue::new(SessionId(1));
    ready(
        &mut q,
        Command::StartContext {
            media: media(),
            start: 0,
            order: Order::Sequential,
        },
    );
    ready(
        &mut q,
        Command::Append(vec![media()[1].clone(), media()[1].clone()]),
    );
    let good = q.checkpoint();
    let mut cases = vec![];
    let mut bad = good.clone();
    bad.manual[1].id = bad.manual[0].id;
    cases.push(bad);
    let mut bad = good.clone();
    bad.order = vec![0, 0, 2];
    cases.push(bad);
    let mut bad = good.clone();
    bad.cursor = Some(99);
    cases.push(bad);
    let mut bad = good.clone();
    bad.current.as_mut().unwrap().context_index = Some(2);
    cases.push(bad);
    let mut bad = good.clone();
    bad.position_ms = 10_001;
    cases.push(bad);
    let mut bad = good;
    bad.next_entry = 0;
    cases.push(bad);
    for bad in cases {
        let before = q.clone();
        assert!(q.restore_checkpoint(bad).is_err());
        assert_eq!(q, before);
    }
}

#[test]
fn checkpoint_preserves_a_stopped_seek_and_rejects_old_load_callbacks() {
    let mut q = Queue::new(SessionId(1));
    q.command(Command::Seek(1234)).unwrap();
    let mut restored = Queue::new(SessionId(2));
    restored.restore_checkpoint(q.checkpoint()).unwrap();
    assert_eq!(restored.position_ms(), 1234);
    let old = q
        .command(Command::StartContext {
            media: media(),
            start: 0,
            order: Order::Sequential,
        })
        .unwrap()
        .into_iter()
        .find_map(|e| match e {
            Effect::Load { request, .. } => Some(request),
            _ => None,
        })
        .unwrap();
    restored.restore_checkpoint(q.checkpoint()).unwrap();
    let new = restored
        .command(Command::Play)
        .unwrap()
        .into_iter()
        .find_map(|e| match e {
            Effect::Load { request, .. } => Some(request),
            _ => None,
        })
        .unwrap();
    restored.event(Event::Ready(old)).unwrap();
    assert_eq!(restored.playback(), Playback::Loading);
    restored.event(Event::Ready(new)).unwrap();
    assert_eq!(restored.playback(), Playback::Playing);
}
