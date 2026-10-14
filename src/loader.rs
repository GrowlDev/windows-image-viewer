//! Background decoding with a small cache around the current image, so arrow keys are instant.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

use eframe::egui::{Context, TextureFilter, TextureHandle, TextureOptions, TextureWrapMode};

use crate::decode::{self, Decoded};

const WORKERS: usize = 2;

/// Still images up to this many pixels keep their pixels in RAM for the colour readout.
const KEEP_PIXELS_BELOW: usize = 4_000_000;

/// Sharp when enlarged, smooth (mipmapped) when reduced.
pub const IMAGE_TEXTURE: TextureOptions = TextureOptions {
    magnification: TextureFilter::Nearest,
    minification: TextureFilter::Linear,
    wrap_mode: TextureWrapMode::ClampToEdge,
    mipmap_mode: Some(TextureFilter::Linear),
};

pub struct Loaded {
    pub info: Decoded,
    pub texture: TextureHandle,
}

pub enum Slot {
    Loading,
    Ready(Arc<Loaded>),
    Failed(String),
}

struct Job {
    path: PathBuf,
    generation: u64,
}

struct Shared {
    queue: Mutex<VecDeque<Job>>,
    wake: Condvar,
    max_side: AtomicUsize,
}

type Done = (PathBuf, u64, Result<Decoded, String>);

pub struct Loader {
    ctx: Context,
    shared: Arc<Shared>,
    done: Receiver<Done>,
    slots: HashMap<PathBuf, Slot>,
    /// Latest request per path; older results for the same path are ignored.
    generations: HashMap<PathBuf, u64>,
    next_generation: u64,
}

impl Loader {
    pub fn new(ctx: Context) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            wake: Condvar::new(),
            max_side: AtomicUsize::new(8192),
        });
        let (tx, done) = channel();
        for i in 0..WORKERS {
            let (shared, tx, ctx) = (shared.clone(), tx.clone(), ctx.clone());
            std::thread::Builder::new()
                .name(format!("decode-{i}"))
                .spawn(move || worker(&shared, &tx, &ctx))
                .expect("spawn decoder thread");
        }
        Self { ctx, shared, done, slots: HashMap::new(), generations: HashMap::new(), next_generation: 0 }
    }

    pub fn set_max_texture_side(&self, side: usize) {
        self.shared.max_side.store(side.max(1024), Ordering::Relaxed);
    }

    pub fn get(&self, path: &Path) -> Option<&Slot> {
        self.slots.get(path)
    }

    /// Keep exactly these images cached, decoding missing ones in this priority order.
    pub fn want(&mut self, paths: &[PathBuf]) {
        self.slots.retain(|p, _| paths.contains(p));
        self.generations.retain(|p, _| paths.contains(p));
        let mut queue = self.shared.queue.lock().unwrap();
        queue.clear();
        for path in paths {
            let missing = !self.slots.contains_key(path);
            let pending = matches!(self.slots.get(path), Some(Slot::Loading));
            if missing || pending {
                let generation = *self.generations.entry(path.clone()).or_insert_with(|| {
                    self.next_generation += 1;
                    self.next_generation
                });
                self.slots.entry(path.clone()).or_insert(Slot::Loading);
                queue.push_back(Job { path: path.clone(), generation });
            }
        }
        drop(queue);
        self.shared.wake.notify_all();
    }

    /// Decode `path` again (it changed on disk). The old picture stays up until the new one lands.
    pub fn reload(&mut self, path: &Path) {
        self.next_generation += 1;
        self.generations.insert(path.to_path_buf(), self.next_generation);
        self.slots.entry(path.to_path_buf()).or_insert(Slot::Loading);
        let job = Job { path: path.to_path_buf(), generation: self.next_generation };
        self.shared.queue.lock().unwrap().push_front(job);
        self.shared.wake.notify_one();
    }

    pub fn forget(&mut self, path: &Path) {
        self.slots.remove(path);
        self.generations.remove(path);
    }

    /// Collect finished decodes. Returns true if anything changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok((path, generation, result)) = self.done.try_recv() {
            if self.generations.get(&path) != Some(&generation) {
                continue;
            }
            let slot = match result {
                Ok(info) => {
                    let name = path.to_string_lossy().into_owned();
                    let texture = self.ctx.load_texture(name, info.frames[0].image.clone(), IMAGE_TEXTURE);
                    let mut info = info;
                    // The texture owns the pixels now. Keep a CPU copy only where it earns its
                    // memory: animation frames, and small images for the colour readout.
                    let [w, h] = info.frames[0].image.size;
                    if !info.is_animated() && w * h > KEEP_PIXELS_BELOW {
                        info.frames.clear();
                    }
                    Slot::Ready(Arc::new(Loaded { info, texture }))
                }
                // A failed re-read (say, the editor is mid-save) keeps the picture we have.
                Err(_) if matches!(self.slots.get(&path), Some(Slot::Ready(_))) => continue,
                Err(e) => Slot::Failed(e),
            };
            self.slots.insert(path, slot);
            changed = true;
        }
        changed
    }
}

fn worker(shared: &Shared, tx: &Sender<Done>, ctx: &Context) {
    loop {
        let job = {
            let mut queue = shared.queue.lock().unwrap();
            loop {
                if let Some(job) = queue.pop_front() {
                    break job;
                }
                queue = shared.wake.wait(queue).unwrap();
            }
        };
        let result = decode::decode(&job.path, shared.max_side.load(Ordering::Relaxed));
        if tx.send((job.path, job.generation, result)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}
