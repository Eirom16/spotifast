//! One authorized encoded block ahead of the decoder. One HTTP owner; seeks
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
pub struct ReadAhead {
    commands: Option<mpsc::SyncSender<(u64, u64)>>,
    replies: Mutex<Option<mpsc::Receiver<io::Result<Block>>>>,
    worker: Option<std::thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    expires: Instant,
    position: u64,
    length: u64,
    block: Block,
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
        let (send, replies) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("doremi-http".into())
            .spawn(move || {
                let mut prepared: Option<(u64, u64, io::Result<Block>)> = None;
                let mut denied = false;
                while let Ok((epoch, at)) = receive.recv() {
                    active.store(epoch, Ordering::Release);
                    let result = if denied {
                        Err(io::ErrorKind::PermissionDenied.into())
                    } else if let Some((_, offset, result)) = prepared
                        .take()
                        .filter(|(generation, offset, _)| *generation == epoch && *offset == at)
                    {
                        debug_assert_eq!(offset, at);
                        result
                    } else {
                        block(&mut self, at)
                    };
                    denied |= result
                        .as_ref()
                        .is_err_and(|e| e.kind() == io::ErrorKind::PermissionDenied);
                    let next = result.as_ref().ok().map(|b| b.at + b.bytes.len() as u64);
                    if send.send(result).is_err() {
                        break;
                    }
                    if let Some(next) = next.filter(|next| *next < length) {
                        let result = block(&mut self, next);
                        denied |= result
                            .as_ref()
                            .is_err_and(|e| e.kind() == io::ErrorKind::PermissionDenied);
                        prepared = Some((epoch, next, result));
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
            position,
            length,
            block: Block {
                at: 0,
                bytes: Vec::new(),
            },
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
        if out.is_empty() || self.position == self.length {
            return Ok(0);
        }
        if self.position < self.block.at
            || self.position - self.block.at >= self.block.bytes.len() as u64
        {
            let at = self.position / CHUNK as u64 * CHUNK as u64;
            let epoch = self.generation.load(Ordering::Acquire);
            // Drop consumed bytes before receiving another block (two retained blocks).
            self.block.bytes = Vec::new();
            self.commands
                .as_ref()
                .ok_or(io::ErrorKind::BrokenPipe)?
                .send((epoch, at))
                .map_err(|_| io::ErrorKind::BrokenPipe)?;
            self.block = self
                .replies
                .get_mut()
                .map_err(|_| io::ErrorKind::BrokenPipe)?
                .as_ref()
                .ok_or(io::ErrorKind::BrokenPipe)?
                .recv()
                .map_err(|_| io::ErrorKind::BrokenPipe)??;
            if self.block.at != at {
                return Err(io::ErrorKind::InvalidData.into());
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
        if position as u64 != self.position {
            self.generation.fetch_add(1, Ordering::AcqRel);
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
    fn check(&self) -> io::Result<()> {
        if (self.cancelled)() {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        if Instant::now() >= self.expires {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }
}
