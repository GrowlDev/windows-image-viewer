//! Animated GIF/APNG/WebP playback.

use super::*;

impl ViewerApp {
    pub(super) fn sync_animation(&mut self, ctx: &egui::Context) {
        let Some(loaded) = self.loaded().filter(|l| l.info.is_animated()) else {
            self.anim = Animation::default();
            return;
        };
        let frames = &loaded.info.frames;
        let mut texture = loaded.texture.clone();
        if self.anim.path != self.current {
            self.anim = Animation { path: self.current.clone(), frame: 0, playing: true, due: None };
            texture.set(frames[0].image.clone(), IMAGE_TEXTURE);
        }
        self.anim.frame = self.anim.frame.min(frames.len() - 1);
        if !self.anim.playing {
            self.anim.due = None;
            return;
        }
        let now = Instant::now();
        let due = *self.anim.due.get_or_insert(now + frames[self.anim.frame].delay);
        if now >= due {
            let mut frame = self.anim.frame;
            let mut next_due = due;
            while now >= next_due {
                if frame + 1 == frames.len() {
                    if !self.looping {
                        self.anim.playing = false;
                        break;
                    }
                    frame = 0;
                } else {
                    frame += 1;
                }
                next_due += frames[frame].delay;
            }
            // After a stall (window hidden, slow decode) restart the clock instead of racing.
            if now.duration_since(due) > Duration::from_secs(1) {
                next_due = now + frames[frame].delay;
            }
            if frame != self.anim.frame {
                self.anim.frame = frame;
                texture.set(frames[frame].image.clone(), IMAGE_TEXTURE);
            }
            self.anim.due = self.anim.playing.then_some(next_due);
        }
        if let Some(due) = self.anim.due {
            ctx.request_repaint_after(due.saturating_duration_since(now));
        }
    }

    pub(super) fn toggle_play(&mut self) {
        let Some(l) = self.loaded().filter(|l| l.info.is_animated()) else { return };
        if !self.anim.playing && self.anim.frame + 1 == l.info.frames.len() && !self.looping {
            self.show_frame(0);
        }
        self.anim.playing = !self.anim.playing;
        self.anim.due = None;
    }

    pub(super) fn toggle_loop(&mut self) {
        self.looping = !self.looping;
    }

    pub(super) fn step_frame(&mut self, step: isize) {
        let Some(l) = self.loaded().filter(|l| l.info.is_animated()) else { return };
        let n = l.info.frames.len() as isize;
        self.anim.playing = false;
        self.show_frame((self.anim.frame as isize + step).rem_euclid(n) as usize);
    }

    pub(super) fn show_frame(&mut self, frame: usize) {
        let Some(l) = self.loaded() else { return };
        if let Some(f) = l.info.frames.get(frame) {
            self.anim.frame = frame;
            self.anim.due = None;
            l.texture.clone().set(f.image.clone(), IMAGE_TEXTURE);
        }
    }
}
