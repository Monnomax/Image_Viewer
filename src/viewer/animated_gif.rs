use gtk4::gdk;
use gtk4::gdk_pixbuf::Pixbuf;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub(super) type AnimatedGifFrames = (Vec<Rc<Pixbuf>>, Vec<Duration>);

struct ActiveGif {
    frames: Vec<Rc<Pixbuf>>,
    delays: Vec<Duration>,
    index: usize,
    next_frame_at: Instant,
    texture: gdk::Texture,
}

#[derive(Default)]
pub(super) struct AnimatedGifPlayer {
    active: Option<ActiveGif>,
    cached_frames: HashMap<PathBuf, AnimatedGifFrames>,
}

impl AnimatedGifPlayer {
    pub(super) fn clear_active(&mut self) {
        self.active = None;
    }

    pub(super) fn cache_frames(&mut self, path: PathBuf, frames: Option<AnimatedGifFrames>) {
        if let Some(frames) = frames {
            self.cached_frames.insert(path, frames);
        } else {
            self.cached_frames.remove(&path);
        }
    }

    pub(super) fn activate(&mut self, path: &Path) {
        let Some((frames, delays)) = self.cached_frames.get(path).cloned() else {
            self.clear_active();
            return;
        };
        if frames.is_empty() || delays.is_empty() {
            self.clear_active();
            return;
        }

        let first = frames[0].clone();
        let first_delay = delays[0];
        self.active = Some(ActiveGif {
            frames,
            delays,
            index: 0,
            next_frame_at: Instant::now() + first_delay,
            texture: gdk::Texture::for_pixbuf(&first),
        });
    }

    pub(super) fn step(&mut self, now: Instant) -> bool {
        let Some(gif) = self.active.as_mut() else {
            return false;
        };
        if now < gif.next_frame_at {
            return false;
        }

        let mut changed = false;
        while now >= gif.next_frame_at {
            gif.index = (gif.index + 1) % gif.frames.len();
            gif.texture = gdk::Texture::for_pixbuf(&gif.frames[gif.index]);
            gif.next_frame_at += gif.delays[gif.index];
            changed = true;
        }
        changed
    }

    pub(super) fn texture(&self) -> Option<gdk::Texture> {
        self.active.as_ref().map(|gif| gif.texture.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_player_does_not_advance() {
        assert!(!AnimatedGifPlayer::default().step(Instant::now()));
    }

    #[test]
    fn empty_frames_do_not_activate_player() {
        let path = PathBuf::from("empty.gif");
        let mut player = AnimatedGifPlayer::default();
        player.cache_frames(path.clone(), Some((vec![], vec![])));
        player.activate(&path);

        assert!(player.active.is_none());
    }

    #[test]
    fn caching_static_image_removes_old_gif_frames() {
        let path = PathBuf::from("image.gif");
        let mut player = AnimatedGifPlayer::default();
        player.cache_frames(path.clone(), Some((vec![], vec![])));
        player.cache_frames(path.clone(), None);

        assert!(!player.cached_frames.contains_key(&path));
    }
}
