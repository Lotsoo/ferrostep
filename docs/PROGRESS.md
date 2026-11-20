# Progress Log — Ferrostep

## 2026-11-21 — Sesi 1: Fase 0 (persiapan lingkungan)

### Selesai
- CLAUDE.md diperbarui: nama proyek **Ferrostep** (crate `ferrostep`), lingkungan dev Fedora 45 KDE/Wayland,
  dance mat *plug and play* jadi tujuan utama (§5.9, dipindah ke Fase 3), kemudahan pengguna (§6.4),
  daftar paket dnf diverifikasi (+ `dbus-devel`, + `evtest`, − `openssl-devel`).
- Pengguna memasang sendiri: paket dnf (§10.1), `dbus-devel`, Rust 1.100.0 via rustup (`~/.cargo/bin` ditambahkan ke PATH fish).
- `cargo init --name ferrostep` + repo Git + `.gitignore` (mengabaikan `catatan.txt` dan `.claude/settings.local.json`).
- Crate: `eframe` 0.37 (backend wgpu default), `kira` 0.12.6, `anyhow`, `log`, `env_logger`.
- Jendela "Halo, Ferrostep!" dengan tema gelap + beep sinus sintetis lewat kira.
  **Terverifikasi oleh pengguna di Fedora 45 KDE Wayland:** jendela muncul, suara terdengar.
  GPU: Intel Iris Xe (Vulkan, Mesa 26.2.6).
- fmt + clippy (`-D warnings`) + test bersih; commit pertama `3de3b26`.

### Catatan teknis
- eframe 0.37: method wajib trait `App` adalah `fn ui(&mut self, ui: &mut egui::Ui, frame)` (bukan `update`).
- kira 0.12: `StaticSoundData` bisa dibuat langsung dari `Arc<[Frame]>` (field publik).
- Fitur default kira `cpal-realtime-dbus` butuh `dbus-devel` di Fedora.
- Warning `libvulkan_dzn.so` (driver Dozen) normal dan tidak berbahaya; log pustaka yang ramai difilter di `main.rs`.

### Berikutnya
- Belum diuji di Windows 11.
- Fase 1: parser `.sm`/`.ssc`, writer `.sm`, modul `timing`, beserta test.

### Masalah terbuka
- Tidak ada.

## 2026-11-21 — Sesi 1 (lanjutan): Fase 1 & 2

### Fase 1 — Parser & timing (selesai)
- `parser/`: pembaca `.sm`/`.ssc` (BOM, CRLF, komentar, tag tak dikenal, `;` hilang, measure 192 baris,
  timing per-chart `.ssc`), writer `.sm` dengan resolusi measure terkecil, test round-trip.
- `timing/`: `TimingMap` (BPM berubah, STOPS, offset; binary search) + `time_notes`.
- Bug yang ditemukan test: tokenizer melewatkan `#` tag berikutnya bila `;` hilang (sudah diperbaiki).

### Fase 2 — Gameplay inti (selesai, perlu uji manual)
- `audio/clock.rs`: AudioClock (interpolasi monoton, batas 50 ms di depan posisi audio).
- `audio/`: musik streaming (kira), volume master, klik metronom.
- `gameplay/`: judgment (jendela StepMania, pengali judge), hold/roll/mine, combo, DP%, life, statistik offset.
- `input/`: keyboard → ColumnEvent/MenuAction (siap untuk gamepad).
- `ui/`: tema (design tokens), font Inter (OFL, ditanam di binary), notefield vektor, efek hit,
  layar Title / Gameplay (lead-in, pause, auto-pause saat jendela tidak fokus) / Results (grafik timing,
  saran kalibrasi satu klik), transisi fade+slide, toast, skor terbaik (`scores.json`).
- `cargo run -- --make-test-song` membuat `Songs/_Test/Metronome/` (WAV sintetis + 3 chart).
