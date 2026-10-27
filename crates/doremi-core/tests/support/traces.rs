// One set of inputs/expected observations, replayed by core and legacy tests.
use doremi_core::media::{Media, MediaId, ProviderId};
use doremi_core::queue::{Command, Order, Queue};

#[derive(Clone, Debug)]
pub enum Step {
    Append(Vec<&'static str>),
    Insert(usize, Vec<&'static str>),
    Move(usize, usize),
    Remove(usize),
    Clear,
    Next,
    Order(Option<Vec<usize>>),
    Context(Vec<&'static str>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Observation {
    pub current: Option<String>,
    pub manual: Vec<String>,
    pub upcoming: Vec<String>,
}

pub fn media(id: &str) -> Media {
    Media {
        id: MediaId::new(ProviderId::new("fixture").unwrap(), id).unwrap(),
        title: id.into(),
        duration_ms: Some(60_000),
    }
}

pub fn initial() -> Queue {
    let mut queue = Queue::default();
    queue
        .command(Command::StartContext {
            media: ["a", "c", "d"].iter().map(|id| media(id)).collect(),
            start: 0,
            order: Order::Sequential,
        })
        .unwrap();
    queue
}

pub fn apply(queue: &mut Queue, step: &Step) {
    let command = match step {
        Step::Append(ids) => Command::Append(ids.iter().map(|id| media(id)).collect()),
        Step::Insert(gap, ids) => Command::Insert {
            gap: *gap,
            media: ids.iter().map(|id| media(id)).collect(),
        },
        Step::Move(from, gap) => Command::Move {
            entry: queue.manual()[*from].id,
            gap: *gap,
        },
        Step::Remove(index) => Command::Remove(queue.manual()[*index].id),
        Step::Clear => Command::ClearManual,
        Step::Next => Command::Next,
        Step::Order(order) => {
            Command::SetOrder(order.clone().map_or(Order::Sequential, Order::Shuffled))
        }
        Step::Context(ids) => Command::StartContext {
            media: ids.iter().map(|id| media(id)).collect(),
            start: 0,
            order: Order::Sequential,
        },
    };
    queue.command(command).unwrap();
}

pub fn observe(queue: &Queue) -> Observation {
    Observation {
        current: queue.current().map(|entry| entry.media.id.local().into()),
        manual: queue
            .manual()
            .iter()
            .map(|entry| entry.media.id.local().into())
            .collect(),
        upcoming: queue
            .upcoming()
            .map(|entry| entry.media.id.local().into())
            .collect(),
    }
}

fn expected(current: &str, manual: &[&str], upcoming: &[&str]) -> Observation {
    Observation {
        current: Some(current.into()),
        manual: manual.iter().map(|id| (*id).into()).collect(),
        upcoming: upcoming.iter().map(|id| (*id).into()).collect(),
    }
}

pub fn traces() -> Vec<Vec<(Step, Observation)>> {
    vec![
        vec![
            (
                Step::Append(vec!["b", "b"]),
                expected("a", &["b", "b"], &["b", "b", "c", "d"]),
            ),
            (Step::Next, expected("b", &["b"], &["b", "c", "d"])),
            (Step::Next, expected("b", &[], &["c", "d"])),
            (Step::Next, expected("c", &[], &["d"])),
        ],
        vec![
            (
                Step::Append(vec!["b", "e", "b"]),
                expected("a", &["b", "e", "b"], &["b", "e", "b", "c", "d"]),
            ),
            (
                Step::Move(0, 3),
                expected("a", &["e", "b", "b"], &["e", "b", "b", "c", "d"]),
            ),
            (
                Step::Remove(1),
                expected("a", &["e", "b"], &["e", "b", "c", "d"]),
            ),
            (
                Step::Insert(1, vec!["x", "x"]),
                expected("a", &["e", "x", "x", "b"], &["e", "x", "x", "b", "c", "d"]),
            ),
            (Step::Clear, expected("a", &[], &["c", "d"])),
        ],
        vec![
            (
                Step::Append(vec!["b"]),
                expected("a", &["b"], &["b", "c", "d"]),
            ),
            (
                Step::Order(Some(vec![0, 2, 1])),
                expected("a", &["b"], &["b", "d", "c"]),
            ),
            (Step::Order(None), expected("a", &["b"], &["b", "c", "d"])),
        ],
        vec![
            (
                Step::Append(vec!["b", "b"]),
                expected("a", &["b", "b"], &["b", "b", "c", "d"]),
            ),
            (
                Step::Context(vec!["x", "y"]),
                expected("x", &["b", "b"], &["b", "b", "y"]),
            ),
            (Step::Clear, expected("x", &[], &["y"])),
        ],
    ]
}
