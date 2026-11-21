//! The greeting video card: plays the videos one after another with no controls.

use super::decoder::{FrameDecoder, FramePack, decode_jpeg};
use super::*;

/// Pause before a video starts (lets the card appear first).
pub(super) const LEAD_IN: f32 = 0.6;
/// The audio position not moving for this long (seconds) means the audio
/// device is stuck; the video then keeps going on the system clock.
pub(super) const AUDIO_STALL: f32 = 0.5;
/// Longest wait (seconds) for the decoder thread before the UI decodes the
/// frame itself.
pub(super) const DECODER_PATIENCE: f32 = 0.25;
/// Pause on the last frame before the next video.
pub(super) const HOLD_END: f32 = 1.0;

/// The greeting video sequence. See module docs.
pub struct Greetings {
    videos: Vec<(&'static str, FramePack, &'static [u8])>,
    index: usize,
    video_since: Instant,
    music: Option<Music>,
    clock: Option<AudioClock>,
    /// Last raw audio position and when it last changed.
    last_raw: (f64, Instant),
    /// The audio stopped advancing: the video continues on the system clock
    /// from (when, video time then), so a stuck audio device never freezes it.
    fallback: Option<(Instant, f64)>,
    texture: Option<TextureHandle>,
    decoder: FrameDecoder,
    /// Frame on screen and when it was uploaded (watchdog for a starved
    /// decoder thread).
    shown_frame: Option<usize>,
    last_upload: Instant,
    /// The decoder thread fell behind once in this video: the UI decodes
    /// every frame itself from now on (about 5 ms per frame).
    decode_here: bool,
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
        let first = videos.first()?.1;
        Some(Self {
            decoder: FrameDecoder::start(first),
            videos,
            index: 0,
            video_since: now,
            music: None,
            clock: None,
            last_raw: (0.0, now),
            fallback: None,
            texture: None,
            shown_frame: None,
            last_upload: now,
            decode_here: false,
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
        if let Some((at, base)) = self.fallback {
            return base + f64::from(anim::since(at, now));
        }
        match (&self.music, &mut self.clock) {
            (Some(music), Some(clock)) => {
                let raw = music.position();
                if raw != self.last_raw.0 {
                    self.last_raw = (raw, now);
                }
                let t = clock.update(raw, now);
                if !music.is_stopped() && anim::since(self.last_raw.1, now) > AUDIO_STALL {
                    log::warn!(
                        "greeting audio stalled at {t:.2} s; video continues on the system clock"
                    );
                    self.fallback = Some((now, t));
                }
                t
            }
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
                    self.last_raw = (music.position(), now);
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
            self.fallback = None;
            self.decoder = FrameDecoder::start(self.videos[self.index].1);
            self.shown_frame = None;
            self.last_upload = now;
            self.decode_here = false;
            // Show the new video's first frame right away: the previous
            // video's last frame must never sit under the new title.
            if let (Some(tex), Some(image)) = (
                &mut self.texture,
                self.videos[self.index].1.frame(0).and_then(decode_jpeg),
            ) {
                tex.set(image, TextureOptions::LINEAR);
                self.shown_frame = Some(0);
            }
            self.ended_at = None;
        }

        // Upload the frame for the current time.
        let pack = &self.videos[self.index].1;
        let frame = ((t.max(0.0) * pack.fps) as usize).min(pack.count - 1);
        let ready = self.decoder.take(frame);
        // Watchdog: if the decoder thread gets no CPU time (a busy machine),
        // decode the needed frame here so the picture never freezes.
        let behind = self.shown_frame != Some(frame);
        if behind && !self.decode_here && anim::since(self.last_upload, now) > DECODER_PATIENCE {
            log::warn!("gift video decoder fell behind; decoding on the UI thread");
            self.decode_here = true;
        }
        let image = match ready {
            Some(found) => Some(found),
            None if behind && self.decode_here => pack
                .frame(frame)
                .and_then(decode_jpeg)
                .map(|img| (frame, img)),
            None => None,
        };
        if let Some((index, image)) = image {
            match &mut self.texture {
                Some(tex) => tex.set(image, TextureOptions::LINEAR),
                None => {
                    self.texture =
                        Some(ctx.load_texture("gift-video", image, TextureOptions::LINEAR))
                }
            }
            self.shown_frame = Some(index);
            self.last_upload = now;
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

    #[allow(clippy::too_many_arguments)] // drawing helper for a single call site
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
        widgets::page_dots(
            &painter,
            pos2(rect.right() - 32.0, rect.top() + 38.0),
            egui::Align::Max,
            n,
            self.index,
            self.index.checked_sub(1),
            slide,
            |i| {
                if i <= self.index {
                    column_color((i % 4) as u8)
                } else {
                    palette.text_dim.gamma_multiply(0.4)
                }
            },
        );

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
