//! Birthday greeting videos shown after the welcome letter (private builds only).
//!
//! `build.rs` converts `private/video/*.mp4` into JPEG frame packs plus Ogg
//! audio and embeds them; public builds embed nothing and never show this.
//! The videos play one after another in a card with no controls at all: no
//! skip, no pause, no seeking, and the window cannot be closed meanwhile.

use std::time::Instant;

use eframe::egui::{
    self, Align2, Color32, ColorImage, Rect, Sense, TextureHandle, TextureOptions, Ui, pos2, vec2,
};
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use super::theme::{self, ACCENT_A, ACCENT_B, Palette, column_color, font, font_bold};
use super::{anim, letter, notefield, widgets};
use crate::audio::{Audio, AudioClock, Music};

/// One embedded greeting video.
pub struct GiftVideo {
    /// Shown as "Ucapan dari <name>".
    pub name: &'static str,
    /// Frame pack written by `build.rs` (`FSV1` format).
    pub frames: &'static [u8],
    /// Ogg Vorbis soundtrack.
    pub audio: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/gift_media.rs"));

/// Pause before a video starts (lets the card appear first).
const LEAD_IN: f32 = 0.6;
/// Pause on the last frame before the next video.
const HOLD_END: f32 = 1.0;

/// Indexed JPEG frames (see `convert_frames` in `build.rs`).
struct FramePack {
    fps: f64,
    count: usize,
    offsets: &'static [u8],
    data: &'static [u8],
}

impl FramePack {
    fn parse(bytes: &'static [u8]) -> Option<Self> {
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

    fn offset(&self, i: usize) -> usize {
        self.offsets
            .get(i * 4..i * 4 + 4)
            .and_then(|b| b.try_into().ok())
            .map_or(0, |b| u32::from_le_bytes(b) as usize)
    }

    fn frame(&self, i: usize) -> Option<&'static [u8]> {
        let i = i.min(self.count - 1);
        self.data.get(self.offset(i)..self.offset(i + 1))
    }

    fn duration(&self) -> f64 {
        self.count as f64 / self.fps
    }
}

fn decode_jpeg(bytes: &[u8]) -> Option<ColorImage> {
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

/// The greeting video sequence. See module docs.
pub struct Greetings {
    videos: Vec<(&'static str, FramePack, &'static [u8])>,
    index: usize,
    video_since: Instant,
    music: Option<Music>,
    clock: Option<AudioClock>,
    texture: Option<TextureHandle>,
    shown_frame: Option<usize>,
    ended_at: Option<Instant>,
    opened: Instant,
    nudged: Option<Instant>,
}

impl Greetings {
    /// `None` when this build has no greeting videos.
    pub fn new(now: Instant) -> Option<Self> {
        let videos: Vec<_> = VIDEOS
            .iter()
            .filter_map(|v| Some((v.name, FramePack::parse(v.frames)?, v.audio)))
            .collect();
        (!videos.is_empty()).then_some(Self {
            videos,
            index: 0,
            video_since: now,
            music: None,
            clock: None,
            texture: None,
            shown_frame: None,
            ended_at: None,
            opened: now,
            nudged: None,
        })
    }

    /// Called when the player tries to close the window.
    pub fn nudge(&mut self, now: Instant) {
        self.nudged = Some(now);
    }

    /// Current playback time of the video in seconds (before start: <= 0).
    fn time(&mut self, now: Instant) -> f64 {
        match (&self.music, &mut self.clock) {
            (Some(music), Some(clock)) => clock.update(music.position(), now),
            _ => f64::from(anim::since(self.video_since, now) - LEAD_IN),
        }
    }

    /// Draws the current video; returns true once every video has finished.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        audio: Option<&mut Audio>,
        now: Instant,
        reduce: bool,
    ) -> bool {
        // Start the soundtrack after the lead-in.
        if self.music.is_none()
            && self.ended_at.is_none()
            && anim::since(self.video_since, now) >= LEAD_IN
            && let Some(audio) = audio
        {
            let (_, _, sound) = &self.videos[self.index];
            match audio.play_bytes(sound, false, 0) {
                Ok(music) => {
                    self.clock = Some(AudioClock::new(music.position(), now));
                    self.music = Some(music);
                }
                Err(e) => log::warn!("gift video audio: {e:#}"),
            }
        }

        let t = self.time(now);
        let duration = self.videos[self.index].1.duration();
        let audio_done = self.music.as_ref().is_some_and(Music::is_stopped);
        if self.ended_at.is_none() && (t >= duration || audio_done) {
            self.ended_at = Some(now);
            if let Some(music) = &mut self.music {
                music.fade_out(150);
            }
        }
        if let Some(end) = self.ended_at
            && anim::since(end, now) >= HOLD_END
        {
            if self.index + 1 >= self.videos.len() {
                return true;
            }
            self.index += 1;
            self.video_since = now;
            self.music = None;
            self.clock = None;
            self.shown_frame = None;
            self.ended_at = None;
        }

        // Upload the frame for the current time.
        let pack = &self.videos[self.index].1;
        let frame = ((t.max(0.0) * pack.fps) as usize).min(pack.count - 1);
        if self.shown_frame != Some(frame)
            && let Some(image) = pack.frame(frame).and_then(decode_jpeg)
        {
            match &mut self.texture {
                Some(tex) => tex.set(image, TextureOptions::LINEAR),
                None => {
                    self.texture =
                        Some(ctx.load_texture("gift-video", image, TextureOptions::LINEAR))
                }
            }
            self.shown_frame = Some(frame);
        }
        let progress = (t / duration).clamp(0.0, 1.0) as f32;
        let remaining = (duration - t.max(0.0)).max(0.0);

        let appear = anim::ease_out_cubic(anim::progress(self.opened, now, 0.5, reduce));
        let screen = ctx.content_rect();
        egui::Modal::new(egui::Id::unique("gift-greetings"))
            .backdrop_color(Color32::from_black_alpha((225.0 * appear) as u8))
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                self.card(
                    ui, palette, screen, appear, progress, remaining, reduce, now,
                );
            });
        ctx.request_repaint();
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn card(
        &self,
        ui: &mut Ui,
        palette: &Palette,
        screen: Rect,
        appear: f32,
        progress: f32,
        remaining: f64,
        reduce: bool,
        now: Instant,
    ) {
        let painter = ui.painter().clone().with_clip_rect(screen);
        if !reduce {
            letter::draw_floating_arrows(&painter, screen, anim::since(self.opened, now));
        }

        // Size: the video fills the card width, the card fits the window.
        let aspect = self.texture.as_ref().map_or(16.0 / 9.0, |t| {
            t.size()[0] as f32 / t.size()[1].max(1) as f32
        });
        let chrome_h = 110.0 + 72.0;
        let max_w = (screen.width() - 64.0).min(1040.0);
        let max_h = screen.height() - 120.0;
        let video_w = (max_w - 64.0).min((max_h - chrome_h) * aspect).max(160.0);
        let video_h = video_w / aspect;
        let size = vec2(video_w + 64.0, video_h + chrome_h);
        let (slot, _) = ui.allocate_exact_size(size, Sense::hover());
        let shake = match self.nudged {
            Some(at) if !reduce && anim::since(at, now) < 0.45 => {
                let s = anim::since(at, now);
                (s * 50.0).sin() * 12.0 * (1.0 - s / 0.45)
            }
            _ => 0.0,
        };
        let rect = Rect::from_center_size(
            slot.center() + vec2(shake, (1.0 - appear) * 30.0),
            slot.size() * (0.94 + 0.06 * appear),
        );

        let accent = column_color((self.index % 4) as u8);
        notefield::draw_glow(&painter, rect.center(), rect.width() * 0.55, ACCENT_A, 0.35);
        painter.rect_filled(rect, theme::RADIUS_CARD, palette.bg_top);
        widgets::card(&painter, rect, palette);
        let stripe_w = (rect.width() - 64.0) / 4.0;
        for c in 0..4u8 {
            let r = Rect::from_min_size(
                pos2(rect.left() + 32.0 + stripe_w * f32::from(c), rect.top()),
                vec2(stripe_w, 4.0),
            );
            painter.rect_filled(r, 0, column_color(c));
        }

        // The new video slides in.
        let slide = if reduce {
            1.0
        } else {
            anim::ease_out_cubic(anim::progress(self.video_since, now, 0.35, false))
        };
        let dx = (1.0 - slide) * 40.0;
        let fade = |c: Color32| c.gamma_multiply(slide);

        // Header: "Ucapan 1 dari 3" and the big title.
        let n = self.videos.len();
        painter.text(
            pos2(rect.left() + 32.0 + dx, rect.top() + 30.0),
            Align2::LEFT_TOP,
            format!("UCAPAN {} DARI {n}", self.index + 1),
            font_bold(14.0),
            fade(palette.text_dim),
        );
        painter.text(
            pos2(rect.left() + 32.0 + dx, rect.top() + 52.0),
            Align2::LEFT_TOP,
            format!("Ucapan dari {}", self.videos[self.index].0),
            font_bold(36.0),
            fade(accent.lerp_to_gamma(Color32::WHITE, 0.25)),
        );
        for i in 0..n {
            let x = rect.right() - 32.0 - (n - 1 - i) as f32 * 22.0;
            let y = rect.top() + 38.0;
            let color = if i <= self.index {
                column_color((i % 4) as u8)
            } else {
                palette.text_dim.gamma_multiply(0.4)
            };
            if i == self.index {
                painter.rect_filled(
                    Rect::from_center_size(pos2(x - 7.0, y), vec2(22.0, 8.0)),
                    4,
                    color,
                );
            } else {
                painter.circle_filled(pos2(x, y), 4.0, color.gamma_multiply(0.7));
            }
        }

        // The video itself.
        let video = Rect::from_min_size(
            pos2(rect.left() + 32.0, rect.top() + 110.0),
            vec2(rect.width() - 64.0, (rect.width() - 64.0) / aspect),
        )
        .translate(vec2(dx, 0.0));
        painter.rect_filled(video, 14, Color32::BLACK);
        if let Some(tex) = &self.texture {
            egui::Image::from_texture((tex.id(), video.size()))
                .corner_radius(14)
                .tint(Color32::WHITE.gamma_multiply(slide))
                .paint_at(ui, video);
        }
        painter.rect_stroke(
            video,
            14,
            egui::Stroke::new(1.5, accent.gamma_multiply(0.5)),
            egui::StrokeKind::Outside,
        );

        // Progress (display only: nothing here can be clicked or dragged).
        let bar_y = video.bottom() + 26.0;
        let bar = Rect::from_min_max(
            pos2(video.left(), bar_y - 3.0),
            pos2(video.right() - 70.0, bar_y + 3.0),
        );
        painter.rect_filled(bar, 3, palette.text_dim.gamma_multiply(0.2));
        if progress > 0.0 {
            let fill = Rect::from_min_max(
                bar.min,
                pos2(bar.left() + bar.width() * progress, bar.max.y),
            );
            widgets::gradient_pill(&painter, fill, ACCENT_A, ACCENT_B);
        }
        let secs = remaining.ceil() as u32;
        painter.text(
            pos2(video.right(), bar_y),
            Align2::RIGHT_CENTER,
            format!("{}:{:02}", secs / 60, secs % 60),
            font_bold(16.0),
            palette.text_dim,
        );

        if let Some(at) = self.nudged {
            let s = anim::since(at, now);
            if s < 2.5 {
                let a = (1.0 - ((s - 2.0) / 0.5).clamp(0.0, 1.0)) * appear;
                painter.text(
                    pos2(rect.center().x, rect.bottom() + 24.0),
                    Align2::CENTER_CENTER,
                    "Tonton sampai habis dulu, ya 🤍",
                    font(18.0),
                    Color32::WHITE.gamma_multiply(a),
                );
            }
        }
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
