//! Import screen: pick or drop MP3/MP4 files, watch the 5-step progress,
//! fix the BPM if needed, and play the new song right away.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::Receiver;
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::{Nav, PlayRequest, difficulty_color, difficulty_label};
use crate::app::Shared;
use crate::audio::Music;
use crate::importer::{ImportEvent, ImportJob, ImportResult, Stage, ffmpeg, is_importable};
use crate::input::{FrameInput, MenuAction};
use crate::timing::TimingMap;
use crate::ui::theme::{self, ACCENT_A, ACCENT_B, DANGER, SUCCESS, font, font_bold};
use crate::ui::{anim, beatfx, widgets};

/// Whether the window can receive dropped files on this platform.
///
/// winit 0.30 only supports file drag-and-drop on X11 (not Wayland) on Linux.
pub fn drop_supported() -> bool {
    !(cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some())
}

enum Phase {
    Idle,
    Running,
    Done(Box<ImportResult>),
    Failed(String),
    /// Several files were imported in one go: show what happened to each.
    Summary,
}

/// Outcome of one file in a multi-file import.
struct BatchEntry {
    /// File name the user picked.
    file: String,
    /// `Ok((song title, bpm))` or `Err(reason)`.
    outcome: Result<(String, f64), String>,
}

pub struct ImportScreen {
    phase: Phase,
    job: Option<ImportJob>,
    queue: Vec<PathBuf>,
    /// File currently being imported.
    current: Option<PathBuf>,
    /// Files in the current run; above 1 the run continues automatically.
    batch_size: usize,
    batch: Vec<BatchEntry>,
    stage: Stage,
    stage_progress: f32,
    /// When each stage finished (for the check-mark animation).
    stage_done: [Option<Instant>; 5],
    waveform: Vec<f32>,
    picker: Option<Receiver<Vec<PathBuf>>>,
    bpm_text: String,
    chart: usize,
    has_ffmpeg: bool,
    started: Instant,
    /// The finished song playing as a preview, so the detected BPM can be
    /// heard and seen pulsing together. Key = (folder, BPM) it was started for.
    preview: Option<(Music, TimingMap, (PathBuf, u64))>,
    beat: beatfx::Beat,
}

impl Drop for ImportScreen {
    fn drop(&mut self) {
        if let Some((m, _, _)) = &mut self.preview {
            m.fade_out(400);
        }
    }
}

impl ImportScreen {
    pub fn new(files: Vec<PathBuf>, shared: &Shared) -> Self {
        let mut s = Self {
            phase: Phase::Idle,
            job: None,
            queue: Vec::new(),
            current: None,
            batch_size: 0,
            batch: Vec::new(),
            stage: Stage::Reading,
            stage_progress: 0.0,
            stage_done: [None; 5],
            waveform: Vec::new(),
            picker: None,
            bpm_text: String::new(),
            chart: 1,
            has_ffmpeg: ffmpeg::find_ffmpeg(shared.settings.ffmpeg_path.as_deref()).is_some(),
            started: Instant::now(),
            preview: None,
            beat: beatfx::Beat::idle(0.0),
        };
        s.enqueue(files);
        s
    }

    /// Whether new files can start right away (nothing running).
    pub fn is_busy(&self) -> bool {
        matches!(self.phase, Phase::Running)
    }

    /// Adds files; unsupported ones are ignored.
    pub fn enqueue(&mut self, files: Vec<PathBuf>) {
        let files: Vec<PathBuf> = files.into_iter().filter(|p| is_importable(p)).collect();
        if files.is_empty() {
            return;
        }
        // Nothing running or waiting: this starts a new run.
        if !self.is_busy() && self.queue.is_empty() {
            self.batch.clear();
            self.batch_size = 0;
        }
        self.batch_size += files.len();
        self.queue.extend(files);
    }

    /// True when several files are being imported in one run.
    fn is_batch(&self) -> bool {
        self.batch_size > 1
    }

    /// In a multi-file run, records the finished file and moves on by itself,
    /// ending on a summary. Single files keep the detailed result card.
    fn advance_batch(&mut self) {
        if !self.is_batch() {
            return;
        }
        let file = self
            .current
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let outcome = match &self.phase {
            Phase::Done(r) => Some(Ok((r.simfile.meta.title.clone(), r.bpm()))),
            Phase::Failed(e) => Some(Err(e.clone())),
            _ => None,
        };
        if let Some(outcome) = outcome {
            self.batch.push(BatchEntry { file, outcome });
            self.current = None;
            self.phase = if self.queue.is_empty() {
                Phase::Summary
            } else {
                Phase::Idle
            };
        } else if matches!(self.phase, Phase::Idle)
            && self.queue.is_empty()
            && !self.batch.is_empty()
        {
            // Cancelled part-way: show what was already imported.
            self.phase = Phase::Summary;
        }
    }

    fn start_next(&mut self, shared: &Shared, ctx: &egui::Context) {
        if self.is_busy() || self.queue.is_empty() {
            return;
        }
        let file = self.queue.remove(0);
        self.current = Some(file.clone());
        self.job = Some(ImportJob::start(
            file,
            &shared.library.root,
            shared.settings.ffmpeg_path.clone(),
            ctx.clone(),
        ));
        self.phase = Phase::Running;
        self.stage = Stage::Reading;
        self.stage_progress = 0.0;
        self.stage_done = [None; 5];
        self.waveform.clear();
        self.started = Instant::now();
    }

    fn open_picker(&mut self, ctx: &egui::Context) {
        if self.picker.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let ctx = ctx.clone();
        // The native dialog blocks, so it runs on its own thread.
        let spawned = std::thread::Builder::new()
            .name("file-dialog".into())
            .spawn(move || {
                let files = rfd::FileDialog::new()
                    .set_title("Pilih lagu untuk diimpor")
                    .add_filter("Audio / video", &crate::importer::IMPORT_EXTENSIONS)
                    .pick_files()
                    .unwrap_or_default();
                let _ = tx.send(files);
                ctx.request_repaint();
            });
        if spawned.is_ok() {
            self.picker = Some(rx);
        }
    }

    fn poll(&mut self, shared: &mut Shared, now: Instant) {
        if let Some(rx) = &self.picker
            && let Ok(files) = rx.try_recv()
        {
            self.picker = None;
            if !files.is_empty() && !files.iter().any(|f| is_importable(f)) {
                shared
                    .toasts
                    .warning("Pilih file MP3, MP4, M4A, OGG, WAV, atau FLAC.");
            }
            self.enqueue(files);
        }
        let Some(job) = &self.job else { return };
        while let Ok(ev) = job.events.try_recv() {
            match ev {
                ImportEvent::Progress(stage, p) => {
                    for s in Stage::ALL.iter().filter(|s| **s < stage) {
                        self.stage_done[s.index()].get_or_insert(now);
                    }
                    self.stage = stage;
                    self.stage_progress = p;
                    if p >= 1.0 {
                        self.stage_done[stage.index()].get_or_insert(now);
                    }
                }
                ImportEvent::Waveform(w) => self.waveform = w,
                ImportEvent::Done(result) => {
                    log::info!("imported {} in {:?}", result.dir.display(), result.elapsed);
                    if !self.is_batch() {
                        shared
                            .toasts
                            .success(format!("“{}” siap dimainkan!", result.simfile.meta.title));
                        shared.sfx(crate::audio::Sfx::Success);
                    }
                    shared.library.mark_new(result.dir.clone());
                    self.bpm_text = format_bpm(result.bpm());
                    self.chart = 1.min(result.simfile.charts.len().saturating_sub(1));
                    self.phase = Phase::Done(result);
                }
                ImportEvent::Failed(e) => self.phase = Phase::Failed(e),
                ImportEvent::Cancelled => {
                    shared.toasts.info("Impor dibatalkan.");
                    self.phase = Phase::Idle;
                }
            }
        }
        if !self.is_busy() {
            self.job = None;
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, input: &FrameInput) -> Option<Nav> {
        let now = input.now;
        let ctx = ui.ctx().clone();
        self.poll(shared, now);
        let was_summary = matches!(self.phase, Phase::Summary);
        self.advance_batch();
        if !was_summary
            && matches!(self.phase, Phase::Summary)
            && self.batch.iter().any(|e| e.outcome.is_ok())
        {
            shared.sfx(crate::audio::Sfx::Success);
        }
        if matches!(self.phase, Phase::Idle) {
            self.start_next(shared, &ctx);
        }
        if self.is_busy() || self.picker.is_some() {
            ctx.request_repaint();
        }

        let mut nav = None;
        for a in &input.menu {
            match (a, &self.phase) {
                (MenuAction::Back, Phase::Running) => self.cancel(),
                (MenuAction::Back, _) => nav = Some(Nav::Title),
                (MenuAction::Confirm, Phase::Idle | Phase::Failed(_)) => self.open_picker(&ctx),
                (MenuAction::Confirm, Phase::Done(r)) => nav = Some(play(r, self.chart)),
                (MenuAction::Left, Phase::Done(_)) => self.chart = self.chart.saturating_sub(1),
                (MenuAction::Right, Phase::Done(r)) => {
                    self.chart = (self.chart + 1).min(r.simfile.charts.len().saturating_sub(1))
                }
                _ => {}
            }
        }

        let rect = ui.max_rect();
        let palette = shared.palette;
        let painter = ui.painter().clone();
        self.update_preview(shared);
        let reduce = shared.settings.reduce_motion;
        self.beat = match &self.preview {
            Some((m, map, _)) if !m.is_stopped() => {
                beatfx::Beat::from_music(map, m.position() - shared.settings.global_offset())
            }
            _ => beatfx::Beat::idle(anim::since(self.started, now)),
        };
        if reduce {
            self.beat = self.beat.calm();
        } else {
            ctx.request_repaint();
        }
        if palette.dark {
            beatfx::draw_highway(&painter, rect, ACCENT_A, &self.beat);
        }
        let content = Rect::from_center_size(
            pos2(rect.center().x, rect.top() + rect.height() * 0.5 + 20.0),
            vec2(
                (rect.width() - 96.0).min(820.0),
                (rect.height() - 160.0).min(560.0),
            ),
        );

        // Header
        let back = Rect::from_min_size(rect.left_top() + vec2(32.0, 28.0), vec2(120.0, 40.0));
        if ui
            .put(back, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "‹  Kembali", back.size())
            })
            .clicked()
        {
            if self.is_busy() {
                self.cancel();
            }
            nav = Some(Nav::Title);
        }
        widgets::text_tracked(
            &painter,
            pos2(back.right() + 24.0, back.center().y),
            Align2::LEFT_CENTER,
            "Impor Lagu",
            font_bold(theme::TEXT_XL),
            palette.text,
        );

        let local = match &self.phase {
            Phase::Idle => self.draw_idle(ui, shared, content),
            Phase::Running => self.draw_running(ui, shared, content, now),
            Phase::Done(_) => self.draw_done(ui, shared, content),
            Phase::Failed(_) => self.draw_failed(ui, shared, content),
            Phase::Summary => self.draw_summary(ui, shared, content),
        };
        nav.or(local)
    }

    /// Stops the running import and drops the queue (app is closing).
    /// Plays the finished song (single imports) and restarts it when the
    /// BPM is corrected, so the pulse always matches the current timing.
    fn update_preview(&mut self, shared: &mut Shared) {
        let wanted = match &self.phase {
            Phase::Done(r) if !self.is_batch() => Some(r),
            _ => None,
        };
        let Some(result) = wanted else {
            if let Some((mut m, _, _)) = self.preview.take() {
                m.fade_out(400);
            }
            return;
        };
        let key = (result.dir.clone(), result.bpm().to_bits());
        if self.preview.as_ref().is_some_and(|(_, _, k)| *k == key) {
            return;
        }
        let meta = &result.simfile.meta;
        let path = result.dir.join(&meta.music);
        let start = self
            .preview
            .take()
            .map(|(mut m, _, _)| {
                let pos = m.position();
                m.fade_out(150);
                pos
            })
            .unwrap_or(meta.sample_start.max(0.0));
        let Some(audio) = shared.audio.as_mut() else {
            return;
        };
        match audio.play_music(&path, start, 600) {
            Ok(music) => {
                self.preview = Some((music, TimingMap::new(&result.simfile.timing), key));
            }
            Err(e) => log::warn!("import preview: {e:#}"),
        }
    }

    pub fn cancel_all(&mut self) {
        self.cancel();
    }

    fn cancel(&mut self) {
        if let Some(job) = &self.job {
            job.cancel();
        }
        self.queue.clear();
    }

    fn draw_idle(&mut self, ui: &mut Ui, shared: &Shared, area: Rect) -> Option<Nav> {
        let palette = shared.palette;
        let zone = Rect::from_min_size(area.min, vec2(area.width(), area.height() * 0.72));
        let response = ui.interact(zone, ui.scope_id().with("drop-zone"), Sense::click());
        let hovering_files = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        let hot = ui
            .ctx()
            .animate_bool(response.id, response.hovered() || hovering_files);
        let painter = ui.painter();
        painter.rect_filled(
            zone,
            theme::RADIUS_CARD,
            palette.surface.lerp_to_gamma(palette.surface_hover, hot),
        );
        dashed_rect(
            painter,
            zone.shrink(10.0),
            palette
                .text_dim
                .lerp_to_gamma(ACCENT_B, hot)
                .gamma_multiply(0.7),
        );

        // Icon: four arrows falling onto a row of targets, one per beat.
        let beat = self.beat;
        let c = zone.center() - vec2(0.0, 56.0);
        let fall = 70.0;
        for col in 0..4u8 {
            let x = c.x + (f32::from(col) - 1.5) * 52.0;
            let target = pos2(x, c.y + 24.0);
            let color = theme::column_color(col);
            // Arrow `col` lands on beat ≡ col (mod 4), so they chase in turn.
            let since = (beat.beat - f64::from(col)).rem_euclid(4.0) as f32;
            let landed = since < 1.0;
            let flash = if landed {
                (1.0 - since).powi(3) * if beat.live { 1.0 } else { 0.6 }
            } else {
                0.0
            };
            if flash > 0.0 {
                crate::ui::notefield::radial_light(painter, target, 46.0, color, 0.8 * flash);
            }
            crate::ui::notefield::draw_arrow(
                painter,
                target,
                40.0,
                col,
                color.gamma_multiply(0.12 + 0.6 * flash),
                Stroke::new(1.5, color.gamma_multiply(0.5 + 0.5 * hot)),
            );
            if !shared.settings.reduce_motion {
                // Falling during the beat before it lands.
                let to_land = 4.0 - since; // beats until next landing
                if to_land < 1.0 || (hot > 0.5 && to_land < 2.0) {
                    let p = 1.0 - to_land.min(1.0);
                    let pos = target - vec2(0.0, fall * (1.0 - p));
                    crate::ui::notefield::draw_arrow(
                        painter,
                        pos,
                        34.0,
                        col,
                        color.gamma_multiply(0.3 + 0.7 * p),
                        Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.6 * p)),
                    );
                }
            }
        }
        // Neon frame that breathes with the beat (stronger while dragging).
        painter.rect_stroke(
            zone.shrink(10.0),
            theme::RADIUS_CARD,
            Stroke::new(
                1.5 + 2.0 * hot,
                ACCENT_B.gamma_multiply((0.12 + 0.3 * hot) * (0.5 + 0.5 * beat.pulse)),
            ),
            StrokeKind::Inside,
        );
        if !shared.settings.reduce_motion {
            ui.ctx().request_repaint();
        }

        let headline = if drop_supported() {
            "Seret MP3/MP4 ke sini atau klik untuk memilih"
        } else {
            "Klik untuk memilih file MP3/MP4"
        };
        painter.text(
            zone.center() + vec2(0.0, 30.0),
            Align2::CENTER_CENTER,
            headline,
            font_bold(theme::TEXT_M),
            palette.text,
        );
        painter.text(
            zone.center() + vec2(0.0, 58.0),
            Align2::CENTER_CENTER,
            "BPM, offset, dan 4 tingkat kesulitan dibuat otomatis • Enter untuk memilih",
            font(theme::TEXT_S),
            palette.text_dim,
        );
        if response.clicked() {
            self.open_picker(ui.ctx());
        }
        if self.picker.is_some() {
            painter.text(
                zone.center_bottom() - vec2(0.0, 36.0),
                Align2::CENTER_CENTER,
                "Menunggu dialog pilih file…",
                font(theme::TEXT_S),
                ACCENT_B,
            );
        }

        // Info line about ffmpeg (friendly, never scary).
        let info = if self.has_ffmpeg {
            "✓ ffmpeg ditemukan: audio dari video disimpan sebagai .ogg yang ringkas."
        } else {
            "ⓘ ffmpeg tidak wajib. Tanpa ffmpeg, audio dari MP4 disimpan sebagai .wav (lebih besar)."
        };
        painter.text(
            pos2(area.center().x, zone.bottom() + 36.0),
            Align2::CENTER_CENTER,
            info,
            font(theme::TEXT_XS + 1.0),
            if self.has_ffmpeg {
                SUCCESS.gamma_multiply(0.85)
            } else {
                palette.text_dim
            },
        );
        painter.text(
            pos2(area.center().x, zone.bottom() + 62.0),
            Align2::CENTER_CENTER,
            format!(
                "Lagu disimpan di {}",
                shared.library.root.join("Imported").display()
            ),
            font(theme::TEXT_XS),
            palette.text_dim.gamma_multiply(0.8),
        );
        None
    }

    fn draw_running(
        &mut self,
        ui: &mut Ui,
        shared: &Shared,
        area: Rect,
        now: Instant,
    ) -> Option<Nav> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        widgets::card(&painter, area, &palette);
        let inner = area.shrink(32.0);
        let name = self
            .job
            .as_ref()
            .and_then(|j| j.source.file_name())
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        painter.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            "Sedang mengimpor",
            font(theme::TEXT_S),
            palette.text_dim,
        );
        widgets::text_tracked(
            &painter,
            inner.left_top() + vec2(0.0, 22.0),
            Align2::LEFT_TOP,
            &super::song_select::truncate(&name, 60),
            font_bold(theme::TEXT_L),
            palette.text,
        );
        if self.is_batch() {
            painter.text(
                inner.right_top(),
                Align2::RIGHT_TOP,
                format!("Lagu {} dari {}", self.batch.len() + 1, self.batch_size),
                font_bold(theme::TEXT_S),
                theme::ACCENT_B,
            );
        }

        // Stepper
        let top = inner.top() + 90.0;
        let step_w = inner.width() / 5.0;
        for (i, stage) in Stage::ALL.iter().enumerate() {
            let center = pos2(inner.left() + step_w * (i as f32 + 0.5), top + 18.0);
            if i > 0 {
                let prev = pos2(center.x - step_w, center.y);
                let done = self.stage_done[i - 1].is_some();
                painter.line_segment(
                    [prev + vec2(22.0, 0.0), center - vec2(22.0, 0.0)],
                    Stroke::new(
                        2.0,
                        if done {
                            ACCENT_B
                        } else {
                            palette.border.gamma_multiply(3.0)
                        },
                    ),
                );
            }
            let done_at = self.stage_done[i];
            let active = *stage == self.stage && done_at.is_none();
            let fill = if done_at.is_some() {
                SUCCESS
            } else if active {
                ACCENT_A
            } else {
                palette.surface_hover
            };
            let pop = done_at.map_or(1.0, |t| {
                anim::ease_out_back(anim::progress(t, now, 0.3, shared.settings.reduce_motion))
            });
            painter.circle_filled(center, 18.0 * pop.max(0.6), fill);
            if active {
                let spin = anim::since(self.started, now) * 4.0;
                painter.add(egui::Shape::line(
                    (0..16)
                        .map(|k| {
                            let a = spin + k as f32 / 15.0 * 4.0;
                            center + vec2(a.cos(), a.sin()) * 23.0
                        })
                        .collect(),
                    Stroke::new(2.5, ACCENT_B),
                ));
            }
            if done_at.is_some() {
                let s = 7.0 * pop;
                painter.line_segment(
                    [center + vec2(-s, 0.0), center + vec2(-s * 0.3, s * 0.7)],
                    Stroke::new(3.0, Color32::from_rgb(12, 12, 20)),
                );
                painter.line_segment(
                    [center + vec2(-s * 0.3, s * 0.7), center + vec2(s, -s * 0.7)],
                    Stroke::new(3.0, Color32::from_rgb(12, 12, 20)),
                );
            } else {
                painter.text(
                    center,
                    Align2::CENTER_CENTER,
                    format!("{}", i + 1),
                    font_bold(theme::TEXT_S),
                    if active {
                        Color32::WHITE
                    } else {
                        palette.text_dim
                    },
                );
            }
            painter.text(
                center + vec2(0.0, 34.0),
                Align2::CENTER_TOP,
                stage.label(),
                font(theme::TEXT_XS + 1.0),
                if active || done_at.is_some() {
                    palette.text
                } else {
                    palette.text_dim
                },
            );
        }

        // Overall progress
        let overall = (self.stage.index() as f32 + self.stage_progress) / 5.0;
        let bar = Rect::from_min_size(pos2(inner.left(), top + 90.0), vec2(inner.width(), 10.0));
        widgets::progress_bar(&painter, bar, overall, &palette);
        painter.text(
            bar.right_bottom() + vec2(0.0, 10.0),
            Align2::RIGHT_TOP,
            format!("{:.0}%", overall * 100.0),
            font(theme::TEXT_S),
            palette.text_dim,
        );

        // Waveform
        let wave = Rect::from_min_max(
            pos2(inner.left(), bar.bottom() + 40.0),
            pos2(inner.right(), inner.bottom() - 70.0),
        );
        draw_waveform(&painter, wave, &self.waveform, overall, &palette);
        if !self.waveform.is_empty() && !shared.settings.reduce_motion {
            // Glowing scanner at the analysis position.
            let x = wave.left() + wave.width() * overall.clamp(0.0, 1.0);
            crate::ui::notefield::radial_light(
                &painter,
                pos2(x, wave.center().y),
                wave.height() * 0.6,
                ACCENT_B,
                0.5 + 0.3 * self.beat.pulse,
            );
            painter.line_segment(
                [pos2(x, wave.top()), pos2(x, wave.bottom())],
                Stroke::new(2.0, Color32::WHITE.gamma_multiply(0.85)),
            );
        }

        let cancel = Rect::from_center_size(
            pos2(inner.center().x, inner.bottom() - 22.0),
            vec2(160.0, 44.0),
        );
        if ui
            .put(cancel, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Batal (Esc)", cancel.size())
            })
            .clicked()
        {
            self.cancel();
        }
        None
    }

    fn draw_done(&mut self, ui: &mut Ui, shared: &mut Shared, area: Rect) -> Option<Nav> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let Phase::Done(result) = &mut self.phase else {
            return None;
        };
        widgets::card(&painter, area, &palette);
        let inner = area.shrink(32.0);
        let meta = &result.simfile.meta;
        painter.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            format!("✓ Selesai dalam {:.1} detik", result.elapsed.as_secs_f32()),
            font(theme::TEXT_S),
            SUCCESS,
        );
        widgets::text_tracked(
            &painter,
            inner.left_top() + vec2(0.0, 24.0),
            Align2::LEFT_TOP,
            &super::song_select::truncate(&meta.title, 48),
            font_bold(theme::TEXT_XL),
            palette.text,
        );
        let artist = if meta.artist.is_empty() {
            "Artis tidak diketahui"
        } else {
            meta.artist.as_str()
        };
        painter.text(
            inner.left_top() + vec2(0.0, 66.0),
            Align2::LEFT_TOP,
            artist,
            font(theme::TEXT_M),
            palette.text_dim,
        );

        // BPM row with ×2 / ÷2 / manual.
        let mut new_bpm = None;
        let row = Rect::from_min_size(
            inner.left_top() + vec2(0.0, 112.0),
            vec2(inner.width(), 44.0),
        );
        painter.text(
            row.left_center(),
            Align2::LEFT_CENTER,
            "BPM terdeteksi",
            font(theme::TEXT_S),
            palette.text_dim,
        );
        let bpm = result.bpm();
        let mut x = row.left() + 130.0;
        let beat = self.beat;
        if beat.live {
            crate::ui::notefield::radial_light(
                &painter,
                pos2(x + 40.0, row.center().y),
                60.0,
                ACCENT_B,
                0.5 * beat.pulse,
            );
        }
        widgets::tabular_text(
            &painter,
            pos2(x, row.center().y),
            Align2::LEFT_CENTER,
            &format_bpm(bpm),
            font_bold(theme::TEXT_L),
            palette.text,
        );
        x += 110.0;
        for (label, factor) in [("÷2", 0.5), ("×2", 2.0)] {
            let r = Rect::from_min_size(pos2(x, row.top()), vec2(56.0, 40.0));
            if ui
                .put(r, |ui: &mut Ui| {
                    widgets::secondary_button(ui, &palette, label, r.size())
                })
                .clicked()
            {
                new_bpm = Some(bpm * factor);
            }
            x += 64.0;
        }
        let field = Rect::from_min_size(pos2(x + 16.0, row.top() + 4.0), vec2(90.0, 32.0));
        let resp = ui.put(
            field,
            egui::TextEdit::singleline(&mut self.bpm_text)
                .font(font(theme::TEXT_M))
                .hint_text("BPM"),
        );
        let apply = Rect::from_min_size(pos2(field.right() + 8.0, row.top()), vec2(100.0, 40.0));
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui
            .put(apply, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Terapkan", apply.size())
            })
            .clicked()
            || enter
        {
            match self.bpm_text.trim().replace(',', ".").parse::<f64>() {
                Ok(v) if (30.0..=400.0).contains(&v) => new_bpm = Some(v),
                _ => shared.toasts.warning("Masukkan BPM antara 30 dan 400."),
            }
        }
        if beat.live {
            beatfx::draw_metronome(
                &painter,
                pos2(apply.right() + 24.0, row.center().y),
                ACCENT_B,
                &beat,
            );
        }
        painter.text(
            pos2(row.left(), row.bottom() + 8.0),
            Align2::LEFT_TOP,
            "Dengarkan lagunya: titik harus berkedip pas di ketukan. Terlalu rapat atau jarang? Coba ÷2 atau ×2.",
            font(theme::TEXT_XS + 1.0),
            palette.text_dim,
        );
        if let Some(b) = new_bpm {
            match result.set_bpm(b) {
                Ok(()) => {
                    self.bpm_text = format_bpm(result.bpm());
                    shared.library.mark_new(result.dir.clone());
                    shared.toasts.success(format!(
                        "Chart dibuat ulang dengan {} BPM",
                        format_bpm(result.bpm())
                    ));
                }
                Err(e) => shared.toasts.warning(format!("Gagal menyimpan: {e}")),
            }
        }

        // Difficulty cards
        let nps = result.chart_nps();
        let cards_top = row.bottom() + 44.0;
        let n = result.simfile.charts.len().max(1);
        let gap = 12.0;
        let w = (inner.width() - gap * (n as f32 - 1.0)) / n as f32;
        for (i, chart) in result.simfile.charts.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(inner.left() + i as f32 * (w + gap), cards_top),
                vec2(w, 96.0),
            );
            let resp = ui.interact(r, ui.scope_id().with(("import-chart", i)), Sense::click());
            if resp.clicked() {
                self.chart = i;
            }
            let sel = self.chart == i;
            let color = difficulty_color(chart.difficulty);
            let hover = ui.ctx().animate_bool(resp.id, resp.hovered() || sel);
            painter.rect_filled(
                r,
                theme::RADIUS_BUTTON,
                color.gamma_multiply(0.08 + 0.1 * hover),
            );
            painter.rect_stroke(
                r,
                theme::RADIUS_BUTTON,
                Stroke::new(
                    if sel { 2.0 } else { 1.0 },
                    color.gamma_multiply(0.4 + 0.6 * hover),
                ),
                StrokeKind::Inside,
            );
            painter.text(
                r.left_top() + vec2(16.0, 14.0),
                Align2::LEFT_TOP,
                difficulty_label(chart.difficulty),
                font_bold(theme::TEXT_S),
                color,
            );
            widgets::text_tracked(
                &painter,
                r.right_top() + vec2(-16.0, 8.0),
                Align2::RIGHT_TOP,
                &chart.meter.to_string(),
                font_bold(theme::TEXT_XL),
                palette.text,
            );
            let rows = count_rows(chart);
            painter.text(
                r.left_bottom() + vec2(16.0, -14.0),
                Align2::LEFT_BOTTOM,
                format!(
                    "{rows} langkah • {:.1}/detik",
                    nps.get(i).copied().unwrap_or(0.0)
                ),
                font(theme::TEXT_XS + 1.0),
                palette.text_dim,
            );
        }

        if let Some(note) = &result.note {
            painter.text(
                pos2(inner.left(), cards_top + 112.0),
                Align2::LEFT_TOP,
                format!("ⓘ {note}"),
                font(theme::TEXT_XS + 1.0),
                palette.text_dim,
            );
        }

        // Live equalizer in the free space, dancing to the preview.
        let eq_top = cards_top + if result.note.is_some() { 140.0 } else { 116.0 };
        let eq = Rect::from_min_max(
            pos2(inner.left(), eq_top),
            pos2(inner.right(), inner.bottom() - 72.0),
        );
        if eq.height() > 24.0 {
            beatfx::draw_equalizer(&painter, eq, (ACCENT_A, ACCENT_B), &beat);
        }

        // Actions
        let y = inner.bottom() - 52.0;
        let play_r = Rect::from_min_size(pos2(inner.left(), y), vec2(240.0, 52.0));
        let mut nav = None;
        if ui
            .put(play_r, |ui: &mut Ui| {
                widgets::primary_button(ui, "▶  Mainkan Sekarang", play_r.size(), true)
            })
            .clicked()
        {
            nav = Some(play(result, self.chart));
        }
        let list_r = Rect::from_min_size(pos2(play_r.right() + 12.0, y + 4.0), vec2(190.0, 44.0));
        if ui
            .put(list_r, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Lihat di daftar lagu", list_r.size())
            })
            .clicked()
        {
            nav = Some(Nav::SongSelect);
        }
        let more = if self.queue.is_empty() {
            "Impor lagu lain".to_string()
        } else {
            format!("Impor berikutnya ({})", self.queue.len())
        };
        let again_r = Rect::from_min_size(pos2(list_r.right() + 12.0, y + 4.0), vec2(190.0, 44.0));
        if ui
            .put(again_r, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, &more, again_r.size())
            })
            .clicked()
        {
            self.phase = Phase::Idle;
        }
        nav
    }

    fn draw_failed(&mut self, ui: &mut Ui, shared: &Shared, area: Rect) -> Option<Nav> {
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let Phase::Failed(msg) = &self.phase else {
            return None;
        };
        let card = Rect::from_center_size(area.center(), vec2(area.width(), 280.0));
        widgets::card(&painter, card, &palette);
        painter.circle_stroke(
            card.center_top() + vec2(0.0, 56.0),
            24.0,
            Stroke::new(3.0, DANGER),
        );
        widgets::text_tracked(
            &painter,
            card.center_top() + vec2(0.0, 56.0),
            Align2::CENTER_CENTER,
            "!",
            font_bold(theme::TEXT_L),
            DANGER,
        );
        widgets::text_tracked(
            &painter,
            card.center_top() + vec2(0.0, 104.0),
            Align2::CENTER_TOP,
            "Lagu ini belum bisa diimpor",
            font_bold(theme::TEXT_L),
            palette.text,
        );
        let galley = painter.layout(
            msg.clone(),
            font(theme::TEXT_S),
            palette.text_dim,
            card.width() - 80.0,
        );
        painter.galley(
            pos2(card.center().x - galley.size().x / 2.0, card.top() + 144.0),
            galley,
            palette.text_dim,
        );
        painter.text(
            card.center_bottom() - vec2(0.0, 92.0),
            Align2::CENTER_CENTER,
            "Coba file lain, atau pastikan file bisa diputar di pemutar musik biasa.",
            font(theme::TEXT_XS + 1.0),
            palette.text_dim,
        );
        let r = Rect::from_center_size(card.center_bottom() - vec2(0.0, 46.0), vec2(200.0, 44.0));
        if ui
            .put(r, |ui: &mut Ui| {
                widgets::primary_button(ui, "Coba file lain", r.size(), true)
            })
            .clicked()
        {
            self.phase = Phase::Idle;
            self.open_picker(ui.ctx());
        }
        None
    }

    fn draw_summary(&mut self, ui: &mut Ui, shared: &Shared, area: Rect) -> Option<Nav> {
        const MAX_ROWS: usize = 7;
        let palette = shared.palette;
        let painter = ui.painter().clone();
        let ok = self.batch.iter().filter(|e| e.outcome.is_ok()).count();
        let failed = self.batch.len() - ok;
        let rows = self.batch.len().min(MAX_ROWS);
        let extra = if self.batch.len() > MAX_ROWS {
            24.0
        } else {
            0.0
        };
        let height = (150.0_f32 + rows as f32 * 40.0 + extra + 76.0).min(area.height());
        let card = Rect::from_center_size(area.center(), vec2(area.width(), height));
        widgets::card(&painter, card, &palette);
        let inner = card.shrink(28.0);

        let title = if failed == 0 {
            format!("{ok} lagu siap dimainkan!")
        } else if ok == 0 {
            "Belum ada lagu yang berhasil diimpor".to_string()
        } else {
            format!("{ok} lagu berhasil, {failed} gagal")
        };
        widgets::text_tracked(
            &painter,
            inner.left_top(),
            Align2::LEFT_TOP,
            &title,
            font_bold(theme::TEXT_L),
            palette.text,
        );
        painter.text(
            inner.left_top() + vec2(0.0, 36.0),
            Align2::LEFT_TOP,
            if ok > 0 {
                "Lagu baru sudah ada di daftar lagu, ditandai badge \"Auto\"."
            } else {
                "Coba file lain, atau pastikan file bisa diputar di pemutar musik biasa."
            },
            font(theme::TEXT_S),
            palette.text_dim,
        );

        let mut y = inner.top() + 76.0;
        for entry in self.batch.iter().take(MAX_ROWS) {
            let row = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 34.0));
            painter.rect_filled(
                row,
                theme::RADIUS_BUTTON,
                palette.surface_hover.gamma_multiply(0.6),
            );
            let icon = pos2(row.left() + 18.0, row.center().y);
            match &entry.outcome {
                Ok((title, bpm)) => {
                    painter.circle_filled(icon, 9.0, SUCCESS.gamma_multiply(0.2));
                    painter.text(icon, Align2::CENTER_CENTER, "✓", font_bold(12.0), SUCCESS);
                    painter.text(
                        pos2(row.left() + 38.0, row.center().y),
                        Align2::LEFT_CENTER,
                        super::song_select::truncate(title, 50),
                        font_bold(theme::TEXT_S),
                        palette.text,
                    );
                    painter.text(
                        pos2(row.right() - 14.0, row.center().y),
                        Align2::RIGHT_CENTER,
                        format!("{} BPM", format_bpm(*bpm)),
                        font(theme::TEXT_XS),
                        palette.text_dim,
                    );
                }
                Err(reason) => {
                    painter.circle_filled(icon, 9.0, DANGER.gamma_multiply(0.2));
                    painter.text(icon, Align2::CENTER_CENTER, "!", font_bold(12.0), DANGER);
                    painter.text(
                        pos2(row.left() + 38.0, row.center().y),
                        Align2::LEFT_CENTER,
                        super::song_select::truncate(&entry.file, 36),
                        font_bold(theme::TEXT_S),
                        palette.text,
                    );
                    painter.text(
                        pos2(row.right() - 14.0, row.center().y),
                        Align2::RIGHT_CENTER,
                        super::song_select::truncate(friendly_reason(reason), 44),
                        font(theme::TEXT_XS),
                        DANGER.lerp_to_gamma(palette.text, 0.3),
                    );
                }
            }
            y += 40.0;
        }
        if self.batch.len() > MAX_ROWS {
            painter.text(
                pos2(inner.left(), y),
                Align2::LEFT_TOP,
                format!("…dan {} lagu lainnya", self.batch.len() - MAX_ROWS),
                font(theme::TEXT_XS),
                palette.text_dim,
            );
        }

        let by = inner.bottom() - 48.0;
        let mut nav = None;
        let list_r = Rect::from_min_size(pos2(inner.left(), by), vec2(220.0, 48.0));
        if ok > 0
            && ui
                .put(list_r, |ui: &mut Ui| {
                    widgets::primary_button(ui, "Lihat di daftar lagu  ›", list_r.size(), true)
                })
                .clicked()
        {
            nav = Some(Nav::SongSelect);
        }
        let again_x = if ok > 0 {
            list_r.right() + 12.0
        } else {
            inner.left()
        };
        let again_r = Rect::from_min_size(pos2(again_x, by + 2.0), vec2(180.0, 44.0));
        if ui
            .put(again_r, |ui: &mut Ui| {
                widgets::secondary_button(ui, &palette, "Impor lagu lain", again_r.size())
            })
            .clicked()
        {
            self.batch.clear();
            self.batch_size = 0;
            self.phase = Phase::Idle;
            self.open_picker(ui.ctx());
        }
        nav
    }
}

fn play(result: &ImportResult, chart: usize) -> Nav {
    Nav::Play(PlayRequest {
        simfile: Arc::new(result.simfile.clone()),
        song_dir: result.dir.clone(),
        chart_index: chart,
    })
}

fn format_bpm(bpm: f64) -> String {
    if (bpm - bpm.round()).abs() < 0.005 {
        format!("{bpm:.0}")
    } else {
        format!("{bpm:.2}")
    }
}

/// Number of steps (rows), counting a jump once.
fn count_rows(chart: &crate::parser::Chart) -> usize {
    let mut beats: Vec<f64> = chart.notes.iter().map(|n| n.beat).collect();
    beats.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    beats.len()
}

/// Dashed rounded-ish border for the drop zone.
fn dashed_rect(painter: &egui::Painter, r: Rect, color: Color32) {
    let stroke = Stroke::new(2.0, color);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    for w in corners.windows(2) {
        painter.extend(egui::Shape::dashed_line(&[w[0], w[1]], stroke, 10.0, 8.0));
    }
}

/// Mirrored waveform; the part already processed is drawn in the accent gradient.
fn draw_waveform(
    painter: &egui::Painter,
    r: Rect,
    peaks: &[f32],
    progress: f32,
    palette: &theme::Palette,
) {
    if peaks.is_empty() {
        // Placeholder shimmer line until decoding finishes.
        painter.line_segment(
            [r.left_center(), r.right_center()],
            Stroke::new(2.0, palette.border.gamma_multiply(3.0)),
        );
        return;
    }
    let max = peaks.iter().copied().fold(1e-6f32, f32::max);
    let w = r.width() / peaks.len() as f32;
    for (i, p) in peaks.iter().enumerate() {
        let x = r.left() + (i as f32 + 0.5) * w;
        let h = (p / max).sqrt() * r.height() * 0.48;
        let frac = i as f32 / peaks.len() as f32;
        let color = if frac <= progress {
            ACCENT_A.lerp_to_gamma(ACCENT_B, frac)
        } else {
            palette.text_dim.gamma_multiply(0.35)
        };
        painter.line_segment(
            [
                pos2(x, r.center().y - h.max(1.0)),
                pos2(x, r.center().y + h.max(1.0)),
            ],
            Stroke::new((w * 0.6).max(1.0), color),
        );
    }
}

/// The plain-language part of an error, without technical details in
/// parentheses or after a colon (those stay in the log).
fn friendly_reason(reason: &str) -> &str {
    reason
        .split(" (")
        .next()
        .and_then(|r| r.split(": ").next())
        .unwrap_or(reason)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::friendly_reason;

    #[test]
    fn strips_technical_details() {
        assert_eq!(
            friendly_reason("format audio tidak dikenali: unsupported feature (probe)"),
            "format audio tidak dikenali"
        );
        assert_eq!(friendly_reason("file kosong"), "file kosong");
    }
}
