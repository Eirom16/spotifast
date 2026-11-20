//! Bounded authorized encoded input ahead of the decoder. One HTTP owner; seeks
//! fence the pending operation before another offset is requested.
use super::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
struct Block {
    at: u64,
    bytes: Vec<u8>,
}
#[derive(Default)]
struct Progress {
    epoch: u64,
    completed: usize,
    finished: bool,
    error: Option<io::ErrorKind>,
}
type Reply = (u64, io::Result<Block>);
// Observed 15 s recoveries cannot fit behind one ~4 s AAC block. Five blocks
// cover the existing 3*6 s + 150 ms recovery budget at the selected 128 kb/s.
// Four queued replies plus the worker's blocked send retain five ahead; with
// the consumer's block this is 384 KiB plus byte guards, excluding HTTP buffers.
const AHEAD: usize = 5;
pub struct ReadAhead {
    commands: Option<mpsc::SyncSender<(u64, u64)>>,
    replies: Mutex<Option<mpsc::Receiver<Reply>>>,
    worker: Option<std::thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    expires: Instant,
    denied: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    prebuffer: bool,
    primed: Option<u64>,
    position: u64,
    length: u64,
    block: Block,
    next: Option<(u64, u64)>,
}
impl Reader {
    pub fn read_ahead(mut self) -> io::Result<ReadAhead> {
        let length = self.grant.length;
        let position = self.position;
        let expires = self.grant.expires;
        let stop = Arc::new(AtomicBool::new(false));
        let generation = Arc::new(AtomicU64::new(0));
        let active = Arc::new(AtomicU64::new(0));
        let original: Arc<dyn Fn() -> bool + Send + Sync> =
            Arc::from(std::mem::replace(&mut self.cancelled, Box::new(|| true)));
        let original_fence = original.clone();
        let cancelled = stop.clone();
        let current = generation.clone();
        let operation = active.clone();
        self.cancelled = Box::new(move || {
            original()
                || cancelled.load(Ordering::Acquire)
                || current.load(Ordering::Acquire) != operation.load(Ordering::Acquire)
        });
        let (commands, receive) = mpsc::sync_channel::<(u64, u64)>(1);
        let (send, replies) = mpsc::sync_channel(AHEAD - 1);
        let worker_generation = generation.clone();
        let worker_stop = stop.clone();
        let denied = Arc::new(AtomicBool::new(false));
        let terminal = denied.clone();
        let progress = Arc::new(Mutex::new(Progress::default()));
        let produced = progress.clone();
        let worker = std::thread::Builder::new()
            .name("doremi-http".into())
            .spawn(move || {
                while let Ok((epoch, mut at)) = receive.recv() {
                    active.store(epoch, Ordering::Release);
                    loop {
                        let result = if terminal.load(Ordering::Acquire) {
                            Err(io::ErrorKind::PermissionDenied.into())
                        } else {
                            block(&mut self, at)
                        };
                        if result
                            .as_ref()
                            .is_err_and(|e| e.kind() == io::ErrorKind::PermissionDenied)
                        {
                            // A definitive refusal is visible even if earlier
                            // authorized blocks are still queued. Never hide
                            // it behind the larger read-ahead window.
                            terminal.store(true, Ordering::Release);
                        }
                        let next = result.as_ref().ok().map(|b| b.at + b.bytes.len() as u64);
                        {
                            let mut progress = produced.lock().unwrap_or_else(|e| e.into_inner());
                            if progress.epoch == epoch {
                                progress.completed += 1;
                                progress.finished = next.is_none_or(|next| next >= length);
                                progress.error = result.as_ref().err().map(io::Error::kind);
                            }
                        }
                        // Already validated replies remain available while the
                        // sole HTTP owner is waiting on a later block.
                        if send.send((epoch, result)).is_err() {
                            return;
                        }
                        if worker_generation.load(Ordering::Acquire) != epoch
                            || worker_stop.load(Ordering::Acquire)
                        {
                            break;
                        }
                        let Some(next) = next.filter(|next| *next < length) else {
                            break;
                        };
                        at = next;
                    }
                }
            })?;
        Ok(ReadAhead {
            commands: Some(commands),
            replies: Mutex::new(Some(replies)),
            worker: Some(worker),
            stop,
            generation,
            cancelled: original_fence,
            expires,
            denied,
            progress,
            prebuffer: false,
            primed: None,
            position,
            length,
            block: Block {
                at: 0,
                bytes: Vec::new(),
            },
            next: None,
        })
    }
}
fn block(reader: &mut Reader, at: u64) -> io::Result<Block> {
    reader.seek(SeekFrom::Start(at))?;
    reader.fill()?;
    Ok(Block {
        at: reader.cached_at,
        bytes: std::mem::take(&mut reader.bytes),
    })
}
impl Read for ReadAhead {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        let epoch = self.generation.load(Ordering::Acquire);
        if self.prebuffer && self.primed != Some(epoch) {
            let progress = self
                .progress
                .lock()
                .map_err(|_| io::ErrorKind::BrokenPipe)?;
            if progress.epoch == epoch
                && let Some(error) = progress.error
            {
                return Err(error.into());
            }
        }
        if out.is_empty() || self.position == self.length {
            return Ok(0);
        }
        if self.position < self.block.at
            || self.position - self.block.at >= self.block.bytes.len() as u64
        {
            let at = self.position / CHUNK as u64 * CHUNK as u64;
            let epoch = self.generation.load(Ordering::Acquire);
            // Drop consumed bytes before receiving another validated block.
            self.block.bytes = Vec::new();
            if self.next != Some((epoch, at)) {
                *self
                    .progress
                    .lock()
                    .map_err(|_| io::ErrorKind::BrokenPipe)? = Progress {
                    epoch,
                    ..Progress::default()
                };
                self.commands
                    .as_ref()
                    .ok_or(io::ErrorKind::BrokenPipe)?
                    .send((epoch, at))
                    .map_err(|_| io::ErrorKind::BrokenPipe)?;
            }
            self.next = None;
            loop {
                let (generation, result) = self
                    .replies
                    .get_mut()
                    .map_err(|_| io::ErrorKind::BrokenPipe)?
                    .as_ref()
                    .ok_or(io::ErrorKind::BrokenPipe)?
                    .recv()
                    .map_err(|_| io::ErrorKind::BrokenPipe)?;
                if generation == epoch {
                    self.block = result?;
                    break;
                }
            }
            if self.block.at != at {
                return Err(io::ErrorKind::InvalidData.into());
            }
            self.next = Some((epoch, at + self.block.bytes.len() as u64));
            if self.prebuffer && self.primed != Some(epoch) {
                // One ~4 s AAC interval cannot cover the observed 9.73 s
                // second GET. Wait for a second result before PCM starts,
                // using the existing bounded window, not another buffer.
                let required = if self.length - at > CHUNK as u64 {
                    2
                } else {
                    1
                };
                loop {
                    self.check()?;
                    let ready = {
                        let progress = self
                            .progress
                            .lock()
                            .map_err(|_| io::ErrorKind::BrokenPipe)?;
                        if progress.epoch == epoch {
                            if let Some(error) = progress.error {
                                return Err(error.into());
                            }
                            progress.completed >= required || progress.finished
                        } else {
                            false
                        }
                    };
                    if ready {
                        break;
                    }
                    if self
                        .worker
                        .as_ref()
                        .is_none_or(|worker| worker.is_finished())
                    {
                        self.progress
                            .lock()
                            .map_err(|_| io::ErrorKind::BrokenPipe)?
                            .error = Some(io::ErrorKind::BrokenPipe);
                        return Err(io::ErrorKind::BrokenPipe.into());
                    }
                    std::thread::park_timeout(Duration::from_millis(20));
                }
                self.primed = Some(epoch);
            }
        }
        self.check()?;
        let index = (self.position - self.block.at) as usize;
        let count = out.len().min(self.block.bytes.len() - index);
        out[..count].copy_from_slice(&self.block.bytes[index..index + count]);
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for ReadAhead {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.check()?;
        let position = match from {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
            SeekFrom::End(n) => i128::from(self.length) + i128::from(n),
        };
        if !(0..=i128::from(self.length)).contains(&position) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        // Demuxers seek backwards over headers and skip forwards inside the
        // current block. Its bytes and the following interval are unchanged:
        // preserve their prefetch, including a seek exactly to that interval.
        let cached = !self.block.bytes.is_empty()
            && (self.block.at..=self.block.at + self.block.bytes.len() as u64)
                .contains(&(position as u64));
        if position as u64 != self.position && !cached {
            self.generation.fetch_add(1, Ordering::AcqRel);
            self.next = None;
            // Free the bounded reply queue, including a worker blocked on
            // send. Tagged late replies are also rejected by read().
            if let Some(replies) = self
                .replies
                .get_mut()
                .map_err(|_| io::ErrorKind::BrokenPipe)?
            {
                while replies.try_recv().is_ok() {}
            }
        }
        self.position = position as u64;
        Ok(self.position)
    }
}
impl Drop for ReadAhead {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.commands.take();
        self.replies
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl ReadAhead {
    /// Buffer two encoded intervals before the first read in each seek
    /// generation. Runs when opening/seeking before the PCM prebuffer clock;
    /// cached demux seeks preserve the generation and its authorized window.
    pub fn with_startup_prebuffer(mut self) -> Self {
        self.prebuffer = true;
        self
    }

    fn check(&self) -> io::Result<()> {
        if (self.cancelled)() {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        if self.denied.load(Ordering::Acquire) || Instant::now() >= self.expires {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    use std::{io::Write, net::TcpListener};

    fn reply(listener: &TcpListener, at: usize, length: usize) {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
            assert!(request.len() < 8192);
        }
        assert!(
            String::from_utf8(request)
                .unwrap()
                .to_ascii_lowercase()
                .contains(&format!("\r\nrange: bytes={at}-{}\r\n", at + CHUNK - 1))
        );
        write!(socket,"HTTP/1.1 206 OK\r\nContent-Length: {CHUNK}\r\nContent-Range: bytes {at}-{}/{length}\r\nConnection: close\r\n\r\n",at+CHUNK-1).unwrap();
        socket.write_all(&vec![(at / CHUNK) as u8; CHUNK]).unwrap();
    }
    fn reader(listener: &TcpListener, length: usize, trace: Arc<Trace>) -> ReadAhead {
        Stream {
            url: format!("http://{}/fixture", listener.local_addr().unwrap()),
            user_agent: UA.into(),
            length: length as u64,
            expires: Instant::now() + Duration::from_secs(20),
            duration_ms: 60000,
        }
        .reader(|| false)
        .unwrap()
        .with_trace(trace)
        .read_ahead()
        .unwrap()
    }
    fn filled(trace: &Trace) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while trace.http_snapshot().len() < AHEAD + 1 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn startup_waits_for_two_intervals_and_repeats_after_an_uncached_seek() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let trace = Arc::new(Trace::default());
        trace.enable_http();
        let input = reader(&listener, 12 * CHUNK, trace.clone()).with_startup_prebuffer();
        let (held, waiting) = mpsc::channel();
        let (release, proceed) = mpsc::channel();
        let server = std::thread::spawn(move || {
            for at in [
                0,
                CHUNK,
                2 * CHUNK,
                3 * CHUNK,
                4 * CHUNK,
                5 * CHUNK,
                10 * CHUNK,
                11 * CHUNK,
            ] {
                // Holding the first/second generation's second response must
                // keep their first byte out of the decoder.
                if at == CHUNK || at == 11 * CHUNK {
                    held.send(()).unwrap();
                    proceed.recv_timeout(Duration::from_secs(3)).unwrap();
                }
                reply(&listener, at, 12 * CHUNK);
            }
        });
        let (complete, completed) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut input = input;
            let mut byte = [255];
            input.read_exact(&mut byte).unwrap();
            complete.send(byte[0]).unwrap();
            filled(&trace);
            input.seek(SeekFrom::Start((10 * CHUNK) as u64)).unwrap();
            input.read_exact(&mut byte).unwrap();
            complete.send(byte[0]).unwrap();
        });
        for expected in [0, 10] {
            waiting.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(matches!(
                completed.recv_timeout(Duration::from_millis(80)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
            release.send(()).unwrap();
            assert_eq!(
                completed.recv_timeout(Duration::from_secs(3)).unwrap(),
                expected
            );
        }
        worker.join().unwrap();
        server.join().unwrap();
    }
    #[test]
    fn startup_prebuffer_cancels_while_the_second_interval_is_blocked() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let input = Stream {
            url: format!("http://{}/fixture", listener.local_addr().unwrap()),
            user_agent: UA.into(),
            length: (2 * CHUNK) as u64,
            expires: Instant::now() + Duration::from_secs(20),
            duration_ms: 60000,
        }
        .reader(move || stop.load(Ordering::Acquire))
        .unwrap()
        .read_ahead()
        .unwrap()
        .with_startup_prebuffer();
        let (held, waiting) = mpsc::channel();
        let server = std::thread::spawn(move || {
            reply(&listener, 0, 2 * CHUNK);
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            held.send(()).unwrap();
            assert_eq!(socket.read(&mut [0; 1]).unwrap(), 0);
        });
        let (complete, completed) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut input = input;
            let mut byte = [255];
            complete
                .send((input.read_exact(&mut byte).unwrap_err().kind(), byte))
                .unwrap();
        });
        waiting.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(matches!(
            completed.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        let started = Instant::now();
        cancel.store(true, Ordering::Release);
        assert_eq!(
            completed.recv_timeout(Duration::from_millis(250)).unwrap(),
            (io::ErrorKind::ConnectionAborted, [255])
        );
        worker.join().unwrap();
        assert!(started.elapsed() < Duration::from_millis(250));
        server.join().unwrap();
    }
    #[test]
    fn one_interval_resource_does_not_wait_for_a_nonexistent_second_block() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut input =
            reader(&listener, CHUNK, Arc::new(Trace::default())).with_startup_prebuffer();
        let server = std::thread::spawn(move || reply(&listener, 0, CHUNK));
        let mut bytes = vec![255; CHUNK];
        input.read_exact(&mut bytes).unwrap();
        assert!(bytes.iter().all(|byte| *byte == 0));
        assert_eq!(input.read(&mut [0; 1]).unwrap(), 0);
        drop(input);
        server.join().unwrap();
    }
    #[test]
    fn invalid_second_interval_fails_startup_without_delivering_cached_audio() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let trace = Arc::new(Trace::default());
        trace.enable_http();
        let mut input = reader(&listener, 2 * CHUNK, trace.clone()).with_startup_prebuffer();
        let server = std::thread::spawn(move || {
            reply(&listener, 0, 2 * CHUNK);
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            write!(socket,"HTTP/1.1 206 OK\r\nContent-Length: {CHUNK}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",CHUNK+1,2*CHUNK,2*CHUNK).unwrap();
            assert_eq!(socket.read(&mut [0; 1]).unwrap(), 0);
        });
        let mut byte = [255];
        assert_eq!(
            input.read_exact(&mut byte).unwrap_err().kind(),
            io::ErrorKind::Other
        );
        assert_eq!(byte, [255]);
        assert_eq!(
            input.read_exact(&mut byte).unwrap_err().kind(),
            io::ErrorKind::Other
        );
        assert_eq!(byte, [255]);
        let reads = trace.http_snapshot();
        assert_eq!(reads.len(), 2);
        assert_eq!(reads[1].outcome, "content_range");
        drop(input);
        server.join().unwrap();
    }
    #[test]
    fn full_window_is_bounded_and_seek_retires_a_blocked_sender() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let trace = Arc::new(Trace::default());
        trace.enable_http();
        let mut input = reader(&listener, 12 * CHUNK, trace.clone());
        let server = std::thread::spawn(move || {
            for index in 0..=AHEAD {
                reply(&listener, index * CHUNK, 12 * CHUNK);
            }
            // The consumer has not advanced; there must be no seventh GET.
            reply(&listener, 10 * CHUNK, 12 * CHUNK);
        });
        input.read_exact(&mut [0; 1]).unwrap();
        filled(&trace);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(trace.http_snapshot().len(), AHEAD + 1);
        let started = Instant::now();
        input.seek(SeekFrom::Start((10 * CHUNK) as u64)).unwrap();
        let mut bytes = [0; 17];
        input.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, [10; 17]);
        assert!(started.elapsed() < Duration::from_millis(500));
        drop(input);
        server.join().unwrap();
    }
    #[test]
    fn queued_bytes_are_readable_while_a_later_http_request_is_blocked() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let trace = Arc::new(Trace::default());
        trace.enable_http();
        let mut input = reader(&listener, 8 * CHUNK, trace.clone());
        let (ready, waiting) = mpsc::channel();
        let server = std::thread::spawn(move || {
            for index in 0..=AHEAD {
                reply(&listener, index * CHUNK, 8 * CHUNK);
            }
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            ready.send(()).unwrap();
            assert_eq!(socket.read(&mut [0; 1]).unwrap(), 0);
        });
        input.read_exact(&mut [0; 1]).unwrap();
        filled(&trace);
        input.read_exact(&mut vec![0; CHUNK - 1]).unwrap();
        let mut bytes = vec![0; CHUNK];
        input.read_exact(&mut bytes).unwrap();
        assert!(bytes.iter().all(|b| *b == 1));
        waiting.recv_timeout(Duration::from_secs(3)).unwrap();
        let started = Instant::now();
        input.read_exact(&mut bytes).unwrap();
        assert!(bytes.iter().all(|b| *b == 2));
        assert!(started.elapsed() < Duration::from_millis(500));
        let started = Instant::now();
        drop(input);
        assert!(started.elapsed() < Duration::from_millis(500));
        server.join().unwrap();
        assert!(trace.http_snapshot().last().unwrap().cancelled);
    }
    #[test]
    fn cached_demux_seeks_preserve_the_active_next_interval() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let trace = Arc::new(Trace::default());
        trace.enable_http();
        let mut input = reader(&listener, 2 * CHUNK, trace.clone());
        let (ready, waiting) = mpsc::channel();
        let (advance, resumed) = mpsc::channel();
        let server = std::thread::spawn(move || {
            reply(&listener, 0, 2 * CHUNK);
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            assert!(
                String::from_utf8(request)
                    .unwrap()
                    .to_ascii_lowercase()
                    .contains(&format!("\r\nrange: bytes={CHUNK}-{}\r\n", 2 * CHUNK - 1))
            );
            ready.send(()).unwrap();
            resumed.recv_timeout(Duration::from_secs(2)).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_millis(100)))
                .unwrap();
            // Header/index seeks within cached bytes must not close HTTP.
            let error = socket.read(&mut [0; 1]).unwrap_err();
            assert!(matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ));
            write!(socket,"HTTP/1.1 206 OK\r\nContent-Length: {CHUNK}\r\nContent-Range: bytes {CHUNK}-{}/{}\r\nConnection: close\r\n\r\n",2*CHUNK-1,2*CHUNK).unwrap();
            socket.write_all(&vec![1; CHUNK]).unwrap();
        });
        input.read_exact(&mut [0; 16]).unwrap();
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        for position in [0, 31, CHUNK as u64 - 1, CHUNK as u64] {
            input.seek(SeekFrom::Start(position)).unwrap();
        }
        advance.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while trace.http_snapshot().len() < 2 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        // No demand for the next block yet: it completed in anticipation.
        assert!(
            trace
                .http_snapshot()
                .iter()
                .all(|r| r.outcome == "validated")
        );
        let mut bytes = [0; 17];
        input.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, [1; 17]);
        drop(input);
        server.join().unwrap();
    }
}
