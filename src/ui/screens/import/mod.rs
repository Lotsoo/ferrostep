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

mod view;

use results::{format_bpm, play};
mod results;

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
