//! A CEF view's images, between receiving a frame and painting it in a window.

use std::sync::Arc;

use gpui::RenderImage;
use smallvec::SmallVec;

#[derive(Default)]
pub struct BrowserFrame {
    image: Option<Arc<RenderImage>>,
    pending: Option<(u32, u32, Vec<u8>)>,
    cleared: bool,
}

impl BrowserFrame {
    pub fn replace(&mut self, width: u32, height: u32, bytes: Vec<u8>) {
        // A tab may receive thousands of frames without being drawn. Never make an image
        // here: only the latest pixels matter until the window actually paints again.
        self.pending = Some((width, height, bytes));
        self.cleared = false;
    }

    pub fn clear(&mut self) {
        self.pending = None;
        self.cleared = true;
    }

    pub fn discard_pending(&mut self) {
        self.pending = None;
    }

    /// Called only from the canvas's paint callback. Eviction goes through that window's
    /// atlas, including when a popup disappeared without a replacement frame.
    pub fn present(&mut self, mut evict: impl FnMut(Arc<RenderImage>)) -> Option<Arc<RenderImage>> {
        if self.cleared {
            if let Some(old) = self.image.take() {
                evict(old);
            }
            self.cleared = false;
        }
        if let Some((width, height, bytes)) = self.pending.take() {
            // gpui's RenderImage is BGRA, as is CEF's on_paint buffer: no conversion.
            let buffer = image::RgbaImage::from_raw(width, height, bytes).expect("frame size");
            let image = Arc::new(RenderImage::new(SmallVec::from_elem(
                image::Frame::new(buffer),
                1,
            )));
            if let Some(old) = self.image.replace(image) {
                evict(old);
            }
        }
        self.image.clone()
    }

    #[cfg(test)]
    fn retained_bytes(&self) -> usize {
        self.image
            .as_ref()
            .map_or(0, |image| image.as_bytes(0).unwrap().len())
            + self.pending.as_ref().map_or(0, |(_, _, bytes)| bytes.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpainted_frames_are_coalesced() {
        // Same path as after_pump, with no gpui draw between frames (a background tab,
        // or a minimized window). Use tiny frames so a regression cannot OOM the test runner.
        let bytes = 32 * 24 * 4;
        let mut frames = BrowserFrame::default();
        for n in 0..1_000 {
            frames.replace(32, 24, vec![n as u8; bytes]);
        }
        assert!(
            frames.retained_bytes() <= bytes * 2,
            "a hidden pane retained {} bytes for {bytes}-byte frames",
            frames.retained_bytes()
        );
    }

    #[test]
    fn only_the_latest_frame_is_materialized_and_the_painted_image_is_evicted() {
        let mut frames = BrowserFrame::default();
        frames.replace(2, 2, vec![1; 16]);
        let first = frames.present(|_| panic!("nothing to evict yet")).unwrap();
        let weak = Arc::downgrade(&first);
        drop(first);
        for n in 0..1_000 {
            frames.replace(2, 2, vec![n as u8; 16]);
            assert_eq!(frames.retained_bytes(), 32);
            assert!(Arc::ptr_eq(
                frames.image.as_ref().unwrap(),
                &weak.upgrade().unwrap()
            ));
        }
        let mut evicted = 0;
        let latest = frames
            .present(|old| {
                assert!(Arc::ptr_eq(&old, &weak.upgrade().unwrap()));
                evicted += 1;
            })
            .unwrap();
        assert_eq!(
            evicted, 1,
            "only an image actually presented needs atlas eviction"
        );
        assert_eq!(latest.as_bytes(0).unwrap(), &[999u16 as u8; 16]);
        assert!(weak.upgrade().is_none(), "old CPU pixels were released too");
        frames.present(|_| panic!("no new frame, no eviction"));
    }

    #[test]
    fn hiding_discards_pending_pixels_and_popup_clear_releases_the_painted_image() {
        let mut frames = BrowserFrame::default();
        frames.replace(2, 2, vec![1; 16]);
        frames.present(|_| {});
        frames.replace(2, 2, vec![2; 16]);
        frames.discard_pending();
        assert_eq!(frames.retained_bytes(), 16);
        assert_eq!(
            frames
                .present(|_| panic!("still the old image"))
                .unwrap()
                .as_bytes(0)
                .unwrap(),
            &[1; 16]
        );
        let mut evicted = 0;
        frames.clear();
        assert!(frames.present(|_| evicted += 1).is_none());
        assert_eq!(evicted, 1);
        assert_eq!(frames.retained_bytes(), 0);
        frames.present(|_| panic!("clear is consumed once"));
    }

    #[test]
    fn full_hd_frames_stay_bounded_without_any_window_draws() {
        let bytes = 1920 * 1080 * 4;
        let mut frames = BrowserFrame::default();
        frames.replace(1920, 1080, vec![0; bytes]);
        frames.present(|_| {});
        // About 2.3 GiB of incoming pixels, without retaining 2.3 GiB or requiring CEF/GPU.
        for n in 0..300 {
            frames.replace(1920, 1080, vec![n as u8; bytes]);
            assert_eq!(frames.retained_bytes(), bytes * 2);
        }
        frames.present(|_| {});
        assert_eq!(frames.retained_bytes(), bytes);
    }
}
