//! Title screen: animated logo and the three main actions.

use std::time::Instant;

use eframe::egui::{self, Align2, Color32, Rect, Stroke, Ui, pos2, vec2};

use super::Nav;
use crate::app::Shared;
use crate::audio::Music;
use crate::input::{FrameInput, MenuAction};
use crate::timing::TimingMap;
use crate::ui::theme::{self, ACCENT_A, ACCENT_B, column_color, font, font_bold};
use crate::ui::{anim, beatfx, widgets};

const ITEMS: [(&str, &str); 4] = [
    ("Main", "Pilih lagu dan mulai bermain"),
    ("Tutorial", "Belajar main langkah demi langkah"),
    ("Impor Lagu", "Ubah MP3/MP4 menjadi chart otomatis"),
    ("Pengaturan", "Kontrol, dance mat, audio, tampilan"),
];

/// Visual tempo of the title animation.
const TITLE_BPM: f32 = 110.0;

/// Background music on the title screen: a library song's preview section.
struct TitleMusic {
    music: Music,
    timing: TimingMap,
    label: String,
}

pub struct TitleScreen {
    selected: usize,
    opened: Instant,
    music: Option<TitleMusic>,
    music_tried: bool,
}

impl Default for TitleScreen {
    fn default() -> Self {
        Self {
            selected: 0,
            opened: Instant::now(),
            music: None,
            music_tried: false,
        }
    }
}

impl Drop for TitleScreen {
    fn drop(&mut self) {
        if let Some(m) = &mut self.music {
            m.music.fade_out(500);
        }
    }
}

impl TitleScreen {
    /// Starts a random song's preview once the library has loaded.
    fn start_music(&mut self, shared: &mut Shared) {
        if self.music_tried || !shared.library.loaded_once || shared.hold_music {
            return;
        }
        self.music_tried = true;
        let Some(audio) = &mut shared.audio else {
            return;
        };
        let with_music: Vec<_> = shared
            .library
            .songs
            .iter()
            .filter(|s| s.music.is_some())
            .collect();
        // Prefer real songs over the generated metronome.
        let real: Vec<_> = with_music
            .iter()
            .filter(|s| s.group != "_Test")
            .copied()
            .collect();
        let pool = if real.is_empty() { with_music } else { real };
        if pool.is_empty() {
            return;
        }
        let pick = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos()) as usize)
            % pool.len();
        let song = pool[pick];
        let Some(path) = &song.music else { return };
        let start = song.simfile.meta.sample_start.max(0.0);
        match audio.play_music(path, start, 1500) {
            Ok(music) => {
                let artist = &song.simfile.meta.artist;
                let label = if artist.is_empty() {
                    song.title()
                } else {
                    format!("{} — {artist}", song.title())
                };
                self.music = Some(TitleMusic {
                    music,
                    timing: TimingMap::new(&song.simfile.timing),
                    label,
                });
            }
            Err(e) => log::warn!("title music: {e:#}"),
        }
    }

    fn choose(&self, i: usize) -> Nav {
        match i {
            0 => Nav::SongSelect,
            1 => Nav::Tutorial,
            2 => Nav::Import(Vec::new()),
            _ => Nav::Settings,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let reduce = shared.settings.reduce_motion;
        if !reduce {
            ui.ctx().request_repaint();
        }
        for a in &input.menu {
            match a {
                MenuAction::Up => self.selected = (self.selected + ITEMS.len() - 1) % ITEMS.len(),
                MenuAction::Down => self.selected = (self.selected + 1) % ITEMS.len(),
                MenuAction::Confirm | MenuAction::Right => return Some(self.choose(self.selected)),
                MenuAction::Back => return Some(Nav::Quit),
                MenuAction::Left => {}
            }
        }

        self.start_music(shared);
        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        let palette = shared.palette;
        let t = if reduce {
            0.0
        } else {
            anim::since(shared.start, input.now)
        };

        // Follow the music's real beat when it plays, otherwise a calm idle tempo.
        let mut beat = match &self.music {
            Some(m) if !m.music.is_stopped() => beatfx::Beat::from_music(
                &m.timing,
                m.music.position() - shared.settings.global_offset(),
            ),
            _ => beatfx::Beat::idle(t * TITLE_BPM / 80.0),
        };
        if reduce {
            beat = beat.calm();
        }
        if palette.dark {
            beatfx::draw_highway(&painter, rect, ACCENT_A, &beat);
        }
        let b = beat.beat as f32;
        let pulse = beat.pulse;
        let logo_center = pos2(rect.left() + rect.width() * 0.32, rect.center().y - 60.0);

        // Shockwave rings on the beat, a bigger one on each bar.
        if !reduce {
            for k in 0..3 {
                let phase = (b + k as f32 / 3.0).rem_euclid(1.0);
                let r = 80.0 + phase * 260.0;
                let alpha = (1.0 - phase).powi(2) * 0.25;
                painter.circle_stroke(
                    logo_center,
                    r,
                    Stroke::new(
                        2.0,
                        ACCENT_A
                            .lerp_to_gamma(ACCENT_B, phase)
                            .gamma_multiply(alpha),
                    ),
                );
            }
            let bar = (b / 4.0).rem_euclid(1.0);
            if beat.live {
                painter.circle_stroke(
                    logo_center,
                    120.0 + bar * 420.0,
                    Stroke::new(
                        4.0 * (1.0 - bar),
                        ACCENT_B.gamma_multiply(0.35 * (1.0 - bar)),
                    ),
                );
            }
        }
        crate::ui::notefield::draw_glow(
            &painter,
            logo_center,
            160.0 + pulse * 30.0,
            ACCENT_A,
            0.6 + pulse * 0.4 + beat.downbeat * 0.3,
        );

        draw_gradient_logo(&painter, logo_center, "Ferrostep", 96.0, t, &beat, reduce);
        painter.text(
            logo_center + vec2(0.0, 70.0),
            Align2::CENTER_CENTER,
            "Injak ketukannya.",
            font(theme::TEXT_M),
            palette.text_dim,
        );

        // Four arrows lighting up in turn, like a chart playing along.
        let current = beat.beat.rem_euclid(4.0).floor() as u8;
        for c in 0..4u8 {
            let center = logo_center + vec2((f32::from(c) - 1.5) * 76.0, 150.0);
            let lit = beat.live && c == current;
            let pop = if lit { pulse } else { 0.0 };
            let color = column_color(c);
            if lit {
                crate::ui::notefield::radial_light(&painter, center, 70.0, color, 0.7 * pop);
            }
            crate::ui::notefield::draw_arrow(
                &painter,
                center,
                54.0 + 10.0 * pop,
                c,
                if lit {
                    color.lerp_to_gamma(Color32::WHITE, 0.35 * pop)
                } else {
                    color.gamma_multiply(0.18)
                },
                Stroke::new(
                    2.0,
                    if lit {
                        Color32::WHITE.gamma_multiply(0.6 + 0.4 * pop)
                    } else {
                        color.gamma_multiply(0.55)
                    },
                ),
            );
        }
        // Equalizer under the arrows.
        beatfx::draw_equalizer(
            &painter,
            Rect::from_center_size(logo_center + vec2(0.0, 232.0), vec2(330.0, 46.0)),
            (ACCENT_A, ACCENT_B),
            &beat,
        );

        // Menu tiles on the right.
        let appear = anim::ease_out_cubic(anim::progress(self.opened, input.now, 0.4, reduce));
        let tile = vec2(420.0, 84.0);
        let x = rect.left() + rect.width() * 0.62;
        let mut nav = None;
        for (i, (title, sub)) in ITEMS.iter().enumerate() {
            let y = rect.center().y - 196.0
                + i as f32 * (tile.y + 14.0)
                + (1.0 - appear) * 30.0 * (i as f32 + 1.0);
            let r = Rect::from_min_size(pos2(x, y), tile);
            if self.selected == i && beat.live {
                // The focused tile breathes on the beat.
                painter.rect_stroke(
                    r.expand(4.0 + 4.0 * pulse),
                    theme::RADIUS_CARD + 4,
                    Stroke::new(
                        2.0 + 3.0 * pulse,
                        ACCENT_B.gamma_multiply(0.15 + 0.45 * pulse),
                    ),
                    egui::StrokeKind::Outside,
                );
            }
            let resp = ui.put(r, |ui: &mut Ui| {
                widgets::menu_tile(ui, &palette, title, sub, tile, self.selected == i)
            });
            if resp.hovered() {
                self.selected = i;
            }
            if resp.clicked() {
                nav = Some(self.choose(i));
            }
        }

        let hints: &[(&str, &str)] = if shared.gamepad_name.is_some() {
            &[("↑ ↓", "Injak untuk memilih"), ("→", "Masuk")]
        } else {
            &[("↑ ↓", "Pilih"), ("Enter", "Masuk"), ("Esc", "Keluar")]
        };
        widgets::key_hints(
            &painter,
            rect.center_bottom() - vec2(0.0, 32.0),
            Align2::CENTER_CENTER,
            hints,
            &palette,
        );
        if let Some(m) = &self.music
            && !m.music.is_stopped()
        {
            // "Now playing" with a tiny live equalizer.
            let eq = Rect::from_min_size(rect.left_bottom() + vec2(24.0, -40.0), vec2(26.0, 22.0));
            beatfx::draw_equalizer(&painter, eq, (ACCENT_A, ACCENT_B), &beat);
            painter.text(
                pos2(eq.right() + 10.0, eq.center().y),
                Align2::LEFT_CENTER,
                super::song_select::truncate(&m.label, 50),
                font_bold(theme::TEXT_XS),
                palette.text,
            );
        }
        if let Some(name) = &shared.gamepad_name {
            painter.text(
                rect.right_top() + vec2(-24.0, 24.0),
                Align2::RIGHT_TOP,
                format!("🎮 {name}"),
                font(theme::TEXT_XS),
                theme::SUCCESS,
            );
        }
        nav
    }
}

/// Draws the logo; letters take colors from a slowly moving gradient and hop
/// one after another on each beat (a wave running through the word).
fn draw_gradient_logo(
    painter: &egui::Painter,
    center: egui::Pos2,
    text: &str,
    size: f32,
    t: f32,
    beat: &beatfx::Beat,
    reduce: bool,
) {
    let scale = 1.0 + if reduce { 0.0 } else { 0.05 * beat.downbeat };
    // Fixed font size, scaled as shapes (an animated size would keep
    // filling the font atlas; see `widgets::scale_shape`).
    let font_id = font_bold(size);
    let widths: Vec<f32> = text
        .chars()
        .map(|c| {
            painter
                .layout_no_wrap(c.to_string(), font_id.clone(), Color32::WHITE)
                .size()
                .x
                * scale
        })
        .collect();
    let total: f32 = widths.iter().sum();
    let mut x = center.x - total / 2.0;
    let n = text.chars().count().max(1) as f32;
    let phase = beat.beat.rem_euclid(1.0) as f32;
    for (i, (c, w)) in text.chars().zip(widths).enumerate() {
        let gradient = ((i as f32 / n) + t * 0.15).sin() * 0.5 + 0.5;
        let color = ACCENT_A.lerp_to_gamma(ACCENT_B, gradient);
        // Each letter hops a little after the previous one.
        let hop = if reduce || !beat.live {
            0.0
        } else {
            let local = phase - i as f32 * 0.04;
            if (0.0..0.3).contains(&local) {
                (local / 0.3 * std::f32::consts::PI).sin() * 10.0
            } else {
                0.0
            }
        };
        let pos = pos2(x + w / 2.0, center.y - hop);
        if hop > 1.0 {
            crate::ui::notefield::radial_light(painter, pos, w, color, 0.25 * hop / 10.0);
        }
        let galley = painter.layout_no_wrap(c.to_string(), font_id.clone(), color);
        let at = Align2::CENTER_CENTER.anchor_size(pos, galley.size());
        painter.add(widgets::scale_shape(
            egui::Shape::galley(at.min, galley, color),
            pos,
            scale,
        ));
        x += w;
    }
}
