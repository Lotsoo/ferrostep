//! Greeting video frames: the embedded frame pack and its background JPEG decoder.

use super::*;

/// Indexed JPEG frames (see `convert_frames` in `build.rs`).
#[derive(Clone, Copy)]
pub(super) struct FramePack {
    pub(super) fps: f64,
    pub(super) count: usize,
    pub(super) offsets: &'static [u8],
    pub(super) data: &'static [u8],
}

impl FramePack {
    pub(super) fn parse(bytes: &'static [u8]) -> Option<Self> {
        let u32_at = |at: usize| -> Option<u32> {
            Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
        };
        if bytes.get(..4)? != b"FSV1" {
            return None;
        }
        let fps = f64::from(u32_at(4)?);
        let count = u32_at(8)? as usize;
        let offsets_end = 12 + (count + 1) * 4;
        let offsets = bytes.get(12..offsets_end)?;
        let data = bytes.get(offsets_end..)?;
        (fps > 0.0 && count > 0).then_some(Self {
            fps,
            count,
            offsets,
            data,
        })
    }

    pub(super) fn offset(&self, i: usize) -> usize {
        self.offsets
            .get(i * 4..i * 4 + 4)
            .and_then(|b| b.try_into().ok())
            .map_or(0, |b| u32::from_le_bytes(b) as usize)
    }

    pub(super) fn frame(&self, i: usize) -> Option<&'static [u8]> {
        let i = i.min(self.count - 1);
        self.data.get(self.offset(i)..self.offset(i + 1))
    }

    pub(super) fn duration(&self) -> f64 {
        self.count as f64 / self.fps
    }
}

/// Decodes one frame; a broken frame (even a decoder panic) gives `None`
/// instead of killing the decoder thread.
pub(super) fn decode_jpeg(bytes: &[u8]) -> Option<ColorImage> {
    std::panic::catch_unwind(|| decode_jpeg_inner(bytes)).unwrap_or_else(|_| {
        log::warn!("gift video frame: decoder panicked");
        None
    })
}

pub(super) fn decode_jpeg_inner(bytes: &[u8]) -> Option<ColorImage> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let pixels = decoder
        .decode()
        .map_err(|e| log::warn!("gift video frame: {e:?}"))
        .ok()?;
    let info = decoder.info()?;
    let size = [usize::from(info.width), usize::from(info.height)];
    (pixels.len() == size[0] * size[1] * 4)
        .then(|| ColorImage::from_rgba_unmultiplied(size, &pixels))
}

/// Decodes one video's frames on a background thread, a few frames ahead of
/// playback, so a slow decode (720p JPEG) never stalls a UI frame.
pub(super) struct FrameDecoder {
    /// Frame the UI wants now; the thread skips ahead if it fell behind.
    pub(super) wanted: Arc<AtomicUsize>,
    pub(super) rx: Receiver<(usize, ColorImage)>,
    /// A decoded frame that is not due yet.
    pub(super) pending: Option<(usize, ColorImage)>,
}

impl FrameDecoder {
    pub(super) fn start(pack: FramePack) -> Self {
        let wanted = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = crossbeam_channel::bounded(4);
        let thread_wanted = Arc::clone(&wanted);
        let spawned = std::thread::Builder::new()
            .name("gift-video".into())
            .spawn(move || {
                let mut next = 0;
                while next < pack.count {
                    next = next.max(thread_wanted.load(Ordering::Relaxed));
                    if let Some(image) = pack.frame(next).and_then(decode_jpeg) {
                        // Fails once the video is over (receiver dropped).
                        if tx.send((next, image)).is_err() {
                            return;
                        }
                    }
                    next += 1;
                }
            });
        if let Err(e) = spawned {
            log::warn!("gift video thread: {e}");
        }
        Self {
            wanted,
            rx,
            pending: None,
        }
    }

    /// Newest decoded frame (index, image) at or before `want`, if a new one
    /// is ready.
    pub(super) fn take(&mut self, want: usize) -> Option<(usize, ColorImage)> {
        self.wanted.store(want, Ordering::Relaxed);
        let mut newest = None;
        while let Some((i, image)) = self.pending.take().or_else(|| self.rx.try_recv().ok()) {
            if i <= want {
                newest = Some((i, image));
            } else {
                self.pending = Some((i, image));
                break;
            }
        }
        newest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frame_pack() {
        // Two fake "frames" of 3 and 2 bytes at 24 fps.
        let mut pack = b"FSV1".to_vec();
        for v in [24u32, 2, 0, 3, 5] {
            pack.extend_from_slice(&v.to_le_bytes());
        }
        pack.extend_from_slice(&[1, 2, 3, 4, 5]);
        let pack: &'static [u8] = Box::leak(pack.into_boxed_slice());
        let p = FramePack::parse(pack).expect("valid pack");
        assert_eq!(p.count, 2);
        assert_eq!(p.frame(0), Some(&[1u8, 2, 3][..]));
        assert_eq!(p.frame(1), Some(&[4u8, 5][..]));
        assert_eq!(p.frame(9), Some(&[4u8, 5][..]), "clamps to the last frame");
        assert!((p.duration() - 2.0 / 24.0).abs() < 1e-9);
        assert!(FramePack::parse(b"NOPE").is_none());
    }
}
