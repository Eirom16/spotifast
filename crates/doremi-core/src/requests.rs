//! Session-scoped routing. Hosts supply monotonic session identities and time.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    Search,
    Resolve,
    ManualEdit,
}

impl Lane {
    fn index(self) -> usize {
        match self {
            Self::Search => 0,
            Self::Resolve => 1,
            Self::ManualEdit => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    session: SessionId,
    generation: u64,
    lane: Lane,
}

impl Ticket {
    pub fn session(self) -> SessionId {
        self.session
    }

    pub fn generation(self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutingError {
    Exhausted,
    SessionNotNewer,
}

/// Cancellation is logical first; the host must also cancel/collect its worker.
/// This owns no queue rows, provider resources or credentials.
#[derive(Clone, Debug)]
pub struct Router {
    session: SessionId,
    serial: u64,
    active: [Option<Ticket>; 3],
}

impl Router {
    pub fn new(session: SessionId) -> Self {
        Self {
            session,
            serial: 0,
            active: [None; 3],
        }
    }

    pub fn session(&self) -> SessionId {
        self.session
    }

    pub fn begin(&mut self, lane: Lane) -> Result<(Ticket, Option<Ticket>), RoutingError> {
        let generation = self.serial.checked_add(1).ok_or(RoutingError::Exhausted)?;
        let ticket = Ticket {
            session: self.session,
            generation,
            lane,
        };
        self.serial = generation;
        let cancelled = self.active[lane.index()].replace(ticket);
        Ok((ticket, cancelled))
    }

    pub fn accepts(&self, ticket: Ticket) -> bool {
        ticket.session == self.session && self.active[ticket.lane.index()] == Some(ticket)
    }

    pub fn finish(&mut self, ticket: Ticket) -> bool {
        if !self.accepts(ticket) {
            return false;
        }
        self.active[ticket.lane.index()] = None;
        true
    }

    pub fn cancel(&mut self, lane: Lane) -> Option<Ticket> {
        self.active[lane.index()].take()
    }

    /// Monotonic across replacement, including restoring a checkpoint in the host.
    pub fn replace_session(&mut self, session: SessionId) -> Result<Vec<Ticket>, RoutingError> {
        if session <= self.session {
            return Err(RoutingError::SessionNotNewer);
        }
        let cancelled = self.active.iter_mut().filter_map(Option::take).collect();
        self.session = session;
        Ok(cancelled)
    }
}
