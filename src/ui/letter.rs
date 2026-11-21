//! Welcome letter shown every time the app starts (private builds only).
//!
//! The text comes from `private/surat.txt` (copied into the build by
//! `build.rs`; empty in public builds, which then never show anything).
//! Format: pages separated by a `---` line; the first line of a page is its
//! heading; `==text==` is highlighted; a line starting with `— ` is the
//! signature; `[[label]]` is the final button's label; lines starting with
//! `#` are comments.
//!
//! The letter must be read to the end: there is no close or skip, the window
//! cannot be closed while it is open, and "Selanjutnya" unlocks after a short
//! reading time the first time a page is shown.

use std::time::Instant;

use eframe::egui::{
    self, Align2, Color32, FontId, Rect, Sense, Stroke, Ui, pos2,
    text::{LayoutJob, TextFormat},
    vec2,
};

use super::theme::{self, ACCENT_A, ACCENT_B, Palette, column_color, font_bold};
use super::{anim, notefield, widgets};
use crate::input::{FrameInput, MenuAction};

const RAW: &str = include_str!(concat!(env!("OUT_DIR"), "/letter.txt"));

const HEADING_SIZE: f32 = 40.0;
const BODY_SIZE: f32 = 25.0;
/// Horizontal padding of a highlight marker (and the gap kept around it).
const MARK_PAD: f32 = 5.0;

/// One page of the letter.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub heading: String,
    /// Body lines; an empty string is a blank line.
    pub lines: Vec<String>,
    pub signature: Option<String>,
    pub button: Option<String>,
}

/// Parses the letter format described in the module docs.
pub fn parse(raw: &str) -> Vec<Page> {
    let mut pages = Vec::new();
    for chunk in raw.split("\n---") {
        let mut page = Page {
            heading: String::new(),
            lines: Vec::new(),
            signature: None,
            button: None,
        };
        for line in chunk.lines() {
            let line = line.trim_end_matches('\r').trim();
            if line.starts_with('#') || line == "---" {
                continue;
            }
            if page.heading.is_empty() {
                if !line.is_empty() {
                    page.heading = line.to_string();
                }
            } else if let Some(sig) = line.strip_prefix("— ") {
                page.signature = Some(format!("— {}", sig.trim()));
            } else if let Some(label) = line.strip_prefix("[[").and_then(|l| l.strip_suffix("]]")) {
                page.button = Some(label.trim().to_string());
            } else {
                page.lines.push(line.to_string());
            }
        }
        while page.lines.last().is_some_and(String::is_empty) {
            page.lines.pop();
        }
        if !page.heading.is_empty() {
            pages.push(page);
        }
    }
    pages
}

/// Splits a line into (text, highlighted) parts on `==`.
fn segments(line: &str) -> Vec<(&str, bool)> {
    line.split("==")
        .enumerate()
        .filter(|(_, s)| !s.is_empty())
        .map(|(i, s)| (s, i % 2 == 1))
        .collect()
}

/// Seconds a page must stay up the first time before "next" unlocks.
fn reading_time(page: &Page) -> f32 {
    let chars: usize =
        page.lines.iter().map(|l| l.chars().count()).sum::<usize>() + page.heading.chars().count();
    (1.0 + chars as f32 * 0.02).clamp(1.5, 4.0)
}

pub struct Letter {
    pages: Vec<Page>,
    page: usize,
    /// Highest page index shown so far (earlier pages need no waiting).
    furthest: usize,
    page_since: Instant,
    /// Direction of the last page change (+1 next, −1 back) for the slide.
    dir: f32,
    opened: Instant,
    /// Last time the player tried to close the window (card shakes).
    nudged: Option<Instant>,
}

impl Letter {
    /// The letter, if this build has one (shown on every start).
    pub fn new(now: Instant) -> Option<Self> {
        let pages = parse(RAW);
        (!pages.is_empty()).then_some(Self {
            pages,
            page: 0,
            furthest: 0,
            page_since: now,
            dir: 1.0,
            opened: now,
            nudged: None,
        })
    }

    /// The player tried to close the window: shake the card instead.
    pub fn nudge(&mut self, now: Instant) {
        self.nudged = Some(now);
    }

    fn ready(&self, now: Instant) -> f32 {
        if self.page < self.furthest {
            return 1.0;
        }
        (anim::since(self.page_since, now) / reading_time(&self.pages[self.page])).clamp(0.0, 1.0)
    }

    fn go(&mut self, to: usize, now: Instant) {
        self.dir = if to > self.page { 1.0 } else { -1.0 };
        self.page = to;
        if to > self.furthest {
            self.furthest = to;
        }
        self.page_since = now;
    }

    /// Draws the letter. Returns true when the player pressed the final button.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        input: &FrameInput,
        reduce: bool,
    ) -> bool {
        let now = input.now;
        let last = self.pages.len() - 1;
        let mut finished = false;
        let ready = self.ready(now) >= 1.0;
        for action in &input.menu {
            match action {
                MenuAction::Left | MenuAction::Up if self.page > 0 => {
                    self.go(self.page - 1, now);
                }
                MenuAction::Right | MenuAction::Down | MenuAction::Confirm if ready => {
                    if self.page < last {
                        self.go(self.page + 1, now);
                    } else {
                        finished = true;
                    }
                }
                _ => {}
            }
        }

        let appear = anim::ease_out_cubic(anim::progress(self.opened, now, 0.5, reduce));
        let screen = ctx.content_rect();
        let modal = egui::Modal::new(egui::Id::unique("welcome-letter"))
            .backdrop_color(Color32::from_black_alpha((215.0 * appear) as u8))
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                self.card(ui, palette, screen, appear, reduce, now)
            });
        ctx.request_repaint();
        finished || modal.inner
    }

    fn card(
        &mut self,
        ui: &mut Ui,
        palette: &Palette,
        screen: Rect,
        appear: f32,
        reduce: bool,
        now: Instant,
    ) -> bool {
        let t = anim::since(self.opened, now);
        let painter = ui.painter().clone().with_clip_rect(screen);
        if !reduce {
            draw_floating_arrows(&painter, screen, t);
        }

        let size = vec2(
            (screen.width() - 64.0).min(860.0),
            (screen.height() - 64.0).min(560.0),
        );
        let (slot, _) = ui.allocate_exact_size(size, Sense::hover());
        let shake = match self.nudged {
            Some(at) if !reduce => {
                let s = anim::since(at, now);
                if s < 0.45 {
                    (s * 50.0).sin() * 12.0 * (1.0 - s / 0.45)
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };
        let rect = Rect::from_center_size(
            slot.center() + vec2(shake, (1.0 - appear) * 30.0),
            slot.size() * (0.94 + 0.06 * appear),
        );

        // Card with a soft glow and a 4-color stripe.
        notefield::draw_glow(&painter, rect.center(), rect.width() * 0.55, ACCENT_A, 0.35);
        // Opaque base so the menu behind never shows through the text.
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

        // Page dots.
        let n = self.pages.len();
        let dots_y = rect.top() + 36.0;
        for i in 0..n {
            let x = rect.center().x + (i as f32 - (n as f32 - 1.0) / 2.0) * 22.0;
            let active = i == self.page;
            let color = if i <= self.furthest {
                column_color((i % 4) as u8)
            } else {
                palette.text_dim.gamma_multiply(0.4)
            };
            if active {
                painter.rect_filled(
                    Rect::from_center_size(pos2(x, dots_y), vec2(22.0, 8.0)),
                    4,
                    color,
                );
            } else {
                painter.circle_filled(pos2(x, dots_y), 4.0, color.gamma_multiply(0.7));
            }
        }

        // Page content slides in from the side it came from.
        let slide = if reduce {
            1.0
        } else {
            anim::ease_out_cubic(anim::progress(self.page_since, now, 0.3, false))
        };
        let content = Rect::from_min_max(
            rect.left_top() + vec2(56.0, 64.0),
            rect.right_bottom() - vec2(56.0, 96.0),
        )
        .translate(vec2((1.0 - slide) * 40.0 * self.dir, 0.0));
        let alpha = slide;
        self.draw_page(&painter, palette, content, alpha);

        // Buttons.
        let mut finished = false;
        let last = self.page + 1 == self.pages.len();
        let button_y = rect.bottom() - 80.0;
        if self.page > 0 {
            let back = Rect::from_min_size(pos2(rect.left() + 40.0, button_y), vec2(170.0, 52.0));
            if ui
                .put(back, |ui: &mut Ui| {
                    widgets::secondary_button(ui, palette, "‹  Kembali", back.size())
                })
                .clicked()
            {
                self.go(self.page - 1, now);
            }
        }
        let label = if last {
            self.pages[self.page]
                .button
                .clone()
                .unwrap_or_else(|| "Selesai".into())
        } else {
            "Selanjutnya  ›".into()
        };
        let next_w = if last { 260.0 } else { 220.0 };
        let next = Rect::from_min_size(
            pos2(rect.right() - 40.0 - next_w, button_y),
            vec2(next_w, 52.0),
        );
        let progress = self.ready(now);
        let ready = progress >= 1.0;
        if last && ready && !reduce {
            // The final button breathes like a heartbeat.
            let beat = (t * 1.6).fract();
            let pulse = (1.0 - beat).powi(4);
            notefield::draw_glow(
                &painter,
                next.center(),
                next.width() * 0.6,
                column_color(0),
                0.3 + 0.5 * pulse,
            );
        }
        let resp = ui.put(next, |ui: &mut Ui| {
            widgets::primary_button(ui, &label, next.size(), ready)
        });
        if !ready {
            // Reading-time bar fills the disabled button.
            let fill = Rect::from_min_size(
                pos2(next.left() + 12.0, next.bottom() - 8.0),
                vec2((next.width() - 24.0) * progress, 3.0),
            );
            widgets::gradient_pill(&painter, fill, ACCENT_A, ACCENT_B);
        }
        if resp.clicked() && ready {
            if last {
                finished = true;
            } else {
                self.go(self.page + 1, now);
            }
        }

        // Gentle reminder when someone tries to close the window.
        if let Some(at) = self.nudged
            && anim::since(at, now) < 2.5
        {
            painter.text(
                pos2(rect.center().x, rect.bottom() + 28.0),
                Align2::CENTER_CENTER,
                "Baca sampai habis dulu, ya 🤍",
                font_bold(theme::TEXT_M),
                Color32::WHITE,
            );
        }
        finished
    }

    fn draw_page(&self, painter: &egui::Painter, palette: &Palette, rect: Rect, alpha: f32) {
        let page = &self.pages[self.page];
        let fade = |c: Color32| c.gamma_multiply(alpha);

        // Heading: big, bold, in the accent gradient color of this page.
        let accent = column_color((self.page % 4) as u8);
        let heading_color = accent.lerp_to_gamma(Color32::WHITE, 0.25);
        let mut job = LayoutJob::default();
        job.wrap.max_width = rect.width();
        job.append(
            &page.heading,
            0.0,
            TextFormat {
                font_id: font_bold(HEADING_SIZE),
                color: fade(heading_color),
                ..Default::default()
            },
        );
        let heading = painter.layout_job(job);
        let mut y = rect.top() + 16.0;
        painter.galley(pos2(rect.left(), y), heading.clone(), Color32::WHITE);
        y += heading.size().y + 22.0;

        // Body: large bold text with highlighted parts.
        let body_font: FontId = font_bold(BODY_SIZE);
        let text_color = if palette.dark {
            Color32::from_rgb(0xF6, 0xF6, 0xFA)
        } else {
            palette.text
        };
        for line in &page.lines {
            if line.is_empty() {
                y += BODY_SIZE * 0.6;
                continue;
            }
            let mut job = LayoutJob::default();
            job.wrap.max_width = rect.width();
            let mut marks = Vec::new();
            let mut chars = 0;
            let mut after_mark = false;
            for (text, highlighted) in segments(line) {
                let len = text.chars().count();
                let format = if highlighted {
                    marks.push(chars..chars + len);
                    TextFormat {
                        font_id: body_font.clone(),
                        color: fade(highlight_text(accent, palette)),
                        line_height: Some(BODY_SIZE * 1.45),
                        ..Default::default()
                    }
                } else {
                    TextFormat {
                        font_id: body_font.clone(),
                        color: fade(text_color),
                        line_height: Some(BODY_SIZE * 1.45),
                        ..Default::default()
                    }
                };
                chars += len;
                // Breathing room so a marker never touches the next word.
                let gap = if (highlighted && chars > len) || after_mark {
                    MARK_PAD
                } else {
                    0.0
                };
                after_mark = highlighted;
                job.append(text, gap, format);
            }
            let galley = painter.layout_job(job);
            let origin = pos2(rect.left(), y);
            draw_marks(painter, &galley, origin, &marks, accent, alpha);
            painter.galley(origin, galley.clone(), Color32::WHITE);
            y += galley.size().y + 10.0;
        }

        if let Some(sig) = &page.signature {
            painter.text(
                pos2(rect.right(), y + 18.0),
                Align2::RIGHT_TOP,
                sig,
                font_bold(BODY_SIZE + 4.0),
                fade(accent.lerp_to_gamma(Color32::WHITE, 0.3)),
            );
        }
    }
}

/// Highlighted words: the column color, brightened so it reads on the card.
fn highlight_text(accent: Color32, palette: &Palette) -> Color32 {
    if palette.dark {
        accent.lerp_to_gamma(Color32::WHITE, 0.2)
    } else {
        accent.lerp_to_gamma(Color32::BLACK, 0.35)
    }
}

/// Paints a soft rounded "marker" behind each highlighted run of characters
/// (`marks` are char ranges of the laid-out line), one piece per wrapped row.
fn draw_marks(
    painter: &egui::Painter,
    galley: &egui::Galley,
    origin: egui::Pos2,
    marks: &[std::ops::Range<usize>],
    accent: Color32,
    alpha: f32,
) {
    if marks.is_empty() {
        return;
    }
    let glyphs: usize = galley.rows.iter().map(|r| r.glyphs.len()).sum();
    if glyphs != galley.job.text.chars().count() {
        return; // Unexpected layout: keep the colored text only.
    }
    let mut index = 0;
    for row in &galley.rows {
        let mut run: Option<Rect> = None;
        let flush = |run: &mut Option<Rect>| {
            if let Some(r) = run.take() {
                let r = r.expand2(vec2(MARK_PAD, 1.0));
                painter.rect_filled(r, 9, accent.gamma_multiply(0.2 * alpha));
                let underline = Rect::from_min_max(
                    pos2(r.left() + 4.0, r.bottom() - 2.5),
                    pos2(r.right() - 4.0, r.bottom()),
                );
                painter.rect_filled(underline, 2, accent.gamma_multiply(0.85 * alpha));
            }
        };
        for g in &row.glyphs {
            let marked = marks.iter().any(|m| m.contains(&index));
            index += 1;
            if !marked {
                flush(&mut run);
                continue;
            }
            // Spaces never start or end a marker; the union bridges inner ones.
            if g.chr.is_whitespace() {
                continue;
            }
            let top = origin.y + row.pos.y + g.pos.y - g.font_ascent;
            let glyph = Rect::from_min_max(
                pos2(origin.x + row.pos.x + g.pos.x, top),
                pos2(origin.x + row.pos.x + g.max_x(), top + g.font_height),
            );
            run = Some(run.map_or(glyph, |r| r.union(glyph)));
        }
        flush(&mut run);
    }
}

/// Soft 4-color arrows drifting upwards behind the card.
pub(super) fn draw_floating_arrows(painter: &egui::Painter, screen: Rect, t: f32) {
    for i in 0..18u32 {
        let seed = (i as f32 * 12.9898).sin() * 43_758.547;
        let fx = seed.fract().abs();
        let speed = 18.0 + 22.0 * ((seed * 1.7).fract().abs());
        let size = 22.0 + 26.0 * ((seed * 3.1).fract().abs());
        let travel = screen.height() + 120.0;
        let y = screen.bottom() + 60.0 - ((t * speed + fx * travel) % travel);
        let x = screen.left() + fx * screen.width() + (t * 0.6 + i as f32).sin() * 20.0;
        let c = (i % 4) as u8;
        notefield::draw_arrow(
            painter,
            pos2(x, y),
            size,
            c,
            column_color(c).gamma_multiply(0.16),
            Stroke::new(1.5, column_color(c).gamma_multiply(0.35)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pages_highlights_signature_and_button() {
        let raw = "# comment\nHai\nLine ==bold== end\n\n---\nTitle 2\nText\n— Rama\n[[Ayo main]]\n";
        let pages = parse(raw);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].heading, "Hai");
        assert_eq!(pages[0].lines, vec!["Line ==bold== end".to_string()]);
        assert_eq!(pages[1].signature.as_deref(), Some("— Rama"));
        assert_eq!(pages[1].button.as_deref(), Some("Ayo main"));
        assert_eq!(
            segments("a ==b== c"),
            vec![("a ", false), ("b", true), (" c", false)]
        );
        assert!(parse("").is_empty());
        assert!(parse("# only comments\n").is_empty());
    }
}
