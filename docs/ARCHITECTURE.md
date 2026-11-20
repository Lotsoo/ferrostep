# Ferrostep architecture

One crate (`ferrostep`), one binary, one process. Core logic (parser, timing,
DSP, chart generation, judgment, input mapping) has no UI dependency and is
covered by `cargo test`. The UI is egui (immediate mode) on wgpu.

## Modules

| Module | Role | Depends on UI? |
|---|---|---|
| `parser` | `.sm`/`.ssc` reader, `.sm` writer, `Simfile` types | no |
| `timing` | beat ↔ seconds (BPMs, stops, offset), `TimedNote` list | no |
| `audio` | kira playback, `Music` handle, metronome clicks | no |
| `audio::clock` | `AudioClock`: smooth, monotonic song time from buffer-rate positions | no |
| `audio_dsp` | symphonia decode, STFT onsets, tempo + phase, synthetic test audio | no |
| `chart_gen` | onsets → 4 charts (quantize, select by NPS, foot model, holds/jumps) | no |
| `importer` | background pipeline file → `Songs/Imported/<Title>/`, ffmpeg helper | egui `Context` only to request repaints |
| `input` | keyboard + gilrs thread → `ColumnEvent` / `MenuAction`; profiles, debounce | egui input events |
| `gameplay` | `GameSession`: judgment, holds, mines, score, life | no |
| `library` | scan `Songs/`, notify hot-reload, scores, test song | egui `Context` for repaints |
| `config` | `Settings` (TOML), config/data folders | no |
| `ui` | theme tokens, widgets, animation helpers, notefield drawing, screens | yes |
| `app` | screen state machine, shared state, transitions, drops, autosave | yes |

## Threads

```
main/UI thread ── egui frame: poll input → screen.ui() → paint
  ├─ input thread      gilrs blocking loop → crossbeam channel → repaint
  ├─ library scanner   scan Songs/ → channel; notify watcher (debounced 500 ms)
  ├─ importer          decode → analyze → charts → write → ImportEvent channel
  ├─ file dialogs      rfd (portal on Linux) → channel
  └─ kira audio thread (cpal)
```

The UI thread never blocks: it drains channels with `try_recv` each frame.
Cancellation uses `Arc<AtomicBool>`.

## Timing and sync

- Note times are computed once (`timing::time_notes`).
- `AudioClock` interpolates kira's playback position with `Instant`, never runs
  backwards and never runs more than ~50 ms ahead of the last audio update.
- `song_time = clock − global_offset`. Input events carry the `Instant` they
  were received; judgment converts them to song time.
- Lead-in uses a monotonic clock and hands over to the audio clock without a jump.

## Import pipeline

1. Read bytes → FNV-1a hash (chart seed).
2. MP3/OGG/WAV/FLAC are copied. MP4/M4A/AAC → `.ogg` via ffmpeg, else `.wav` via hound.
   When ffmpeg converted, the **converted** file is analyzed so AAC encoder-delay
   handling can never shift the offset.
3. STFT (Hann 2048 / hop 512) → log spectral flux (full, low, high band) →
   adaptive peak picking on a low-band-balanced function.
4. Tempo: weighted autocorrelation (preference around 120 BPM) → phase-coherence
   refinement → beat phase on a 1 ms grid weighted towards kicks.
5. `chart_gen` per difficulty; deterministic ChaCha8 RNG.
6. Everything is written into a hidden `.import-*` folder and renamed at the end.

## Screens

`Title`, `Onboarding`, `SongSelect`, `Import`, `Gameplay`, `Results`,
`Settings` (with `Calibration`), `PadSetup`. Each screen's `ui()` returns an
optional `Nav`; `app` performs the transition.

## Motion and materials (`ui/anim.rs`, `ui/widgets.rs`)

Follows Apple's fluid-interface principles, adapted to egui:

- `anim::Spring` (damping ratio + response, no fixed duration) is interruptible:
  a new target keeps the current value and velocity. `SMOOTH` (1.0 / 0.35) is the
  default; `MOMENTUM` (0.8 / 0.35) only after a flick.
- Screen transitions slide 48 px along the navigation direction (deeper = from the
  right, back = from the left). Reduced motion: a 150 ms cross-fade only.
- Song Select list: wheel/keys move a spring target; drag tracks 1:1 with
  rubber-band edges (`anim::rubberband`), and release hands the pointer velocity
  to the spring and projects the landing point (`anim::project`, rate 0.998).
  One focus highlight slides between cards; edges fade instead of cutting hard.
- Controls react on press (3 % shrink + darker fill), not on release.
- Surfaces have soft elevation shadows (`widgets::elevation`, larger cards are
  deeper) and a faint top highlight. `widgets::text_tracked` applies
  size-aware letter spacing (tight for large titles, open for small caps labels).

## Known platform limits

- File drag-and-drop: winit 0.30 supports X11/Windows/macOS, not Wayland.
  The import zone switches to "click to choose" on Wayland.
- On Wayland the compositor paces frames to the monitor refresh rate even with
  VSync off.
