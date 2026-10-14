//! Gallery thumbnails: decoded on their own threads, only for cards on screen, kept in a
//! bounded cache.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

use eframe::egui::{ColorImage, Context, TextureHandle};

use crate::loader::IMAGE_TEXTURE;

const WORKERS: usize = 4;
/// Longest side of a stored thumbnail. Smaller images keep their exact pixels.
pub const MAX_SIDE: u32 = 256;
const CAPACITY: usize = 600;

pub struct Thumb {
    pub texture: TextureHandle,
    /// The image's real size (the texture may be smaller).
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
}

pub enum Slot {
    Pending,
    Ready(Thumb),
    Failed,
}

struct Shared {
    queue: Mutex<VecDeque<PathBuf>>,
    wake: Condvar,
}

struct Done {
    path: PathBuf,
    result: Result<(ColorImage, u32, u32, bool), String>,
}

pub struct Thumbs {
    ctx: Context,
    shared: Arc<Shared>,
    done: Receiver<Done>,
    slots: HashMap<PathBuf, (Slot, u64)>,
    frame: u64,
}

impl Thumbs {
    pub fn new(ctx: Context) -> Self {
        let shared = Arc::new(Shared { queue: Mutex::new(VecDeque::new()), wake: Condvar::new() });
        let (tx, done) = channel();
        for i in 0..WORKERS {
            let (shared, tx, ctx) = (shared.clone(), tx.clone(), ctx.clone());
            std::thread::Builder::new()
                .name(format!("thumb-{i}"))
                .spawn(move || worker(&shared, &tx, &ctx))
                .expect("spawn thumbnail thread");
        }
        Self { ctx, shared, done, slots: HashMap::new(), frame: 0 }
    }

    /// The cards visible this frame, in display order. Missing ones are queued; nothing else is.
    pub fn show(&mut self, visible: &[PathBuf]) {
        self.frame += 1;
        let mut queue = self.shared.queue.lock().unwrap();
        queue.clear();
        for path in visible {
            match self.slots.get_mut(path) {
                Some((Slot::Pending, used)) => {
                    *used = self.frame;
                    queue.push_back(path.clone());
                }
                Some((_, used)) => *used = self.frame,
                None => {
                    self.slots.insert(path.clone(), (Slot::Pending, self.frame));
                    queue.push_back(path.clone());
                }
            }
        }
        drop(queue);
        self.shared.wake.notify_all();
        self.evict();
    }

    pub fn get(&self, path: &Path) -> Option<&Slot> {
        self.slots.get(path).map(|(s, _)| s)
    }

    /// The file changed on disk.
    pub fn invalidate(&mut self, path: &Path) {
        self.slots.remove(path);
    }

    pub fn poll(&mut self) {
        while let Ok(Done { path, result }) = self.done.try_recv() {
            // Dropped from the cache (scrolled far away) or invalidated while decoding.
            let Some((slot, _)) = self.slots.get_mut(&path) else { continue };
            *slot = match result {
                Ok((image, width, height, has_alpha)) => {
                    let texture = self.ctx.load_texture(format!("thumb:{}", path.display()), image, IMAGE_TEXTURE);
                    Slot::Ready(Thumb { texture, width, height, has_alpha })
                }
                Err(_) => Slot::Failed,
            };
        }
    }

    fn evict(&mut self) {
        if self.slots.len() <= CAPACITY {
            return;
        }
        let mut by_age: Vec<(u64, PathBuf)> = self.slots.iter().map(|(p, (_, used))| (*used, p.clone())).collect();
        by_age.sort();
        for (used, path) in by_age.into_iter().take(self.slots.len() - CAPACITY) {
            if used < self.frame {
                self.slots.remove(&path);
            }
        }
    }
}

fn worker(shared: &Shared, tx: &Sender<Done>, ctx: &Context) {
    loop {
        let path = {
            let mut queue = shared.queue.lock().unwrap();
            loop {
                if let Some(p) = queue.pop_front() {
                    break p;
                }
                queue = shared.wake.wait(queue).unwrap();
            }
        };
        let result = crate::decode::thumbnail(&path, MAX_SIDE);
        if tx.send(Done { path, result }).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}
