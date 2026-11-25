//! Publish the existing artwork cache to desktop media controls. Windows
//! needs a file URI and Linux can show the cover without another HTTP request.
use super::*;
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};
use tempfile::{Builder, NamedTempFile};

type Input = (u64, Arc<egui::ColorImage>);
type ImageFile = (u64, Option<PathBuf>);

struct Worker {
    work: Option<SyncSender<Input>>,
    result: Receiver<ImageFile>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[derive(Default)]
pub(super) struct Cover {
    worker: Option<Worker>,
    pending: Option<u64>,
    file: Option<ImageFile>,
}

impl Cover {
    pub fn update(&mut self, image: Option<Input>, ctx: &egui::Context) -> Option<PathBuf> {
        if let Some(worker) = &self.worker {
            match worker.result.try_recv() {
                Ok(image) => {
                    self.pending = None;
                    self.file = Some(image);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let (version, pixels) = image?;
        if let Some((_, file)) = self.file.as_ref().filter(|(known, _)| *known == version) {
            return file.clone();
        }
        if self.pending.is_none() {
            if self.worker.is_none() {
                self.worker = Worker::start();
            }
            if let Some(work) = self.worker.as_ref().and_then(|worker| worker.work.as_ref())
                && work.try_send((version, pixels)).is_ok()
            {
                self.pending = Some(version);
            }
        }
        if self.pending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(75));
        }
        None
    }
}

impl Worker {
    fn start() -> Option<Self> {
        let (work, requests) = sync_channel(1);
        let (results, result) = channel();
        let thread = std::thread::Builder::new()
            .name("doremi-media-art".into())
            .spawn(move || export(requests, results))
            .ok()?;
        Some(Self {
            work: Some(work),
            result,
            thread: Some(thread),
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.work.take();
        // The owner drops this after the desktop event loop exits. In the
        // disconnected case the worker has already exited.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn export(requests: Receiver<Input>, results: Sender<ImageFile>) {
    let mut files = std::collections::VecDeque::with_capacity(2);
    while let Ok((version, image)) = requests.recv() {
        let path = save_png(&image).ok().map(|file| {
            let path = file.path().to_owned();
            if files.len() == 2 {
                files.pop_front();
            }
            files.push_back(file);
            path
        });
        if results.send((version, path)).is_err() {
            break;
        }
    }
}

fn save_png(image: &egui::ColorImage) -> std::io::Result<NamedTempFile> {
    use image::ImageEncoder;
    let mut file = Builder::new()
        .prefix("doremi-media-art-")
        .suffix(".png")
        .tempfile()?;
    let rgba: Vec<_> = image
        .pixels
        .iter()
        .flat_map(|pixel| pixel.to_srgba_unmultiplied())
        .collect();
    image::codecs::png::PngEncoder::new(file.as_file_mut())
        .write_image(
            &rgba,
            image.size[0] as u32,
            image.size[1] as u32,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(std::io::Error::other)?;
    Ok(file)
}
