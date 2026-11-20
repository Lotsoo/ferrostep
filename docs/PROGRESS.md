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

### Fase 3 — Dance mat + Library & Song Select (selesai, perlu uji dengan perangkat)
- `input/gamepad.rs`: gilrs di thread sendiri (membangunkan UI saat mat diinjak; timestamp event asli dipakai),
  hot-plug, debounce per panel (tekan pertama tidak tertunda), axis/hat → tekan, pemetaan default D-pad.
- `input/profiles.rs`: profil per perangkat (VID:PID:nama) di `devices.json`, kode mentah (tombol/axis±).
- Wizard "Atur Dance Mat" otomatis muncul saat mat tanpa profil dicolok (bisa "Nanti saja"), uji panel langsung.
- Auto-pause + toast saat mat dicabut; toast saat dicolok.
- `library/`: pemindaian `Songs/<Grup>/<Lagu>/` di background, `.ssc` diutamakan, hot-reload `notify` (debounce 500 ms),
  lagu contoh dibuat otomatis saat folder Songs belum ada.
- Layar Pilih Lagu: kartu, pencarian instan, chip tingkat kesulitan, skor terbaik, preview audio dengan fade,
  latar dari hash judul, skeleton saat memindai, layar kosong dengan ajakan impor.
- Catatan: mapping_source gilrs tidak bisa membedakan perangkat "dikenal", jadi aturan: belum ada profil → wizard.
- Belum: gambar banner (butuh crate image) — latar memakai gradien dari hash judul.

### Fase 4 — audio_dsp (selesai)
- `decode.rs`: symphonia 0.6 (versi sama dengan kira) → mono f32 pada sample rate asli, baca tag judul/artis,
  opsi tulis WAV stereo 16-bit sambil decode, bisa dibatalkan.
- `onset.rs`: STFT Hann 2048/hop 512, spectral flux log, pita low/high, peak picking adaptif (median ±100 ms + delta),
  interpolasi parabola sub-frame, latensi flux dihitung analitis (3N/4 − H/2 sampel), sustain, jendela paling energik.
- `tempo.rs`: autokorelasi berbobot (preferensi 120 BPM), penghalusan BPM via phase coherence (resolusi 0,0002),
  pembulatan bila dekat bilangan bulat, fase grid 1 ms dengan bobot kick.
- Hasil test sintetis: 128/140/95,5 BPM klik dan drum 120 BPM → BPM tepat (±0,01), error fase ≤ 5 ms.
- Pelajaran: hi-hat/noise mendominasi flux total sehingga kick hilang → onset dipilih dari fungsi deteksi
  seimbang (½ flux total + ½ flux pita rendah), fase memakai bobot pita rendah.

### Fase 5 — chart_gen + importer (selesai)
- `chart_gen`: kuantisasi onset ke grid per difficulty, seleksi berdasarkan skor dengan batas NPS & jarak minimum,
  model kaki (kiri: L/D/U, kanan: D/U/R, bergantian → tanpa crossover), batas jack ≤ 2 dari model,
  jump hanya di beat kuat (tidak di Beginner), hold hanya Medium/Hard (satu hold sekaligus), tanpa note di 1 detik pertama,
  meter 1–10 dari NPS rata-rata + puncak, RNG ChaCha8 di-seed hash FNV-1a file (deterministik).
  Property test: 25 seed acak × 4 difficulty tanpa pelanggaran aturan ergonomis.
- `importer`: thread latar, 5 tahap, batal via AtomicBool, ditulis ke folder `.import-*` lalu di-rename (tidak ada
  lagu setengah jadi), nama folder disanitasi + sufiks ` (2)`. MP3/OGG/WAV/FLAC disalin; MP4/M4A/AAC → `.ogg` via
  ffmpeg (config → folder exe → PATH), fallback `.wav`. Koreksi BPM ×2/÷2/manual menulis ulang chart.
- Layar Import: zona drop, stepper animasi, progress bar, waveform, kartu hasil (BPM, 4 chart + meter + langkah),
  "Mainkan Sekarang", antrean banyak file. Drop file di layar mana pun → Import; overlay "Lepaskan untuk mengimpor".
- Ukuran (build release, laptop dev): MP3 4 menit → 0,4 s; MP4 4 menit dengan ffmpeg → 2,3 s; tanpa ffmpeg → 0,5 s.
  Target < 30 s tercapai jauh.
- Pelajaran: decoder AAC berbeda soal pemangkasan *encoder delay* (~21 ms). Karena itu bila ffmpeg dipakai,
  analisis dilakukan pada `.ogg` hasil konversi, bukan file sumber, agar offset cocok dengan audio yang diputar.

**Keterbatasan diketahui:** winit 0.30 belum mendukung drag-and-drop file di **Wayland** (hanya X11/Windows/macOS).
Di sesi Wayland, zona impor otomatis berganti teks "Klik untuk memilih file" (dialog lewat XDG portal via `rfd`).

### Fase 6 — Settings, kalibrasi, onboarding, polish (selesai)
- Layar **Pengaturan** dengan sidebar (Gameplay, Audio, Tampilan, Kontrol, Folder), tersimpan otomatis:
  CMod/XMod + kecepatan, reverse, tingkat penilaian (Longgar/Normal/Ketat), No Fail, kecerahan latar, volume,
  global offset (slider ±5), tema gelap/terang & skala UI (langsung diterapkan), kurangi animasi, FPS,
  VSync & backend grafis (berlaku setelah restart), **keybind** (klik → tekan tombol, klik kanan hapus, default),
  uji panel live, daftar dance mat + "Atur ulang dance mat", debounce, panduan "Dance mat tidak terbaca?" per OS,
  folder lagu (buka/ganti/default/pindai ulang), ffmpeg (status/pilih/otomatis).
- **Wizard kalibrasi offset** (`calibration.rs`): metronom 100 BPM, 4 ketukan hitung-masuk, 16 ketukan,
  statistik robust (buang 10 % terburuk), usulan offset + kestabilan. Dipakai di Settings dan onboarding.
- **Onboarding pertama kali**: Kontrol (panah menyala saat ditekan, status mat) → Kalibrasi (bisa dilewati)
  → Mainkan lagu contoh / Impor lagu pertama. "Lewati semua" selalu ada.
- **Layar judul**: memutar preview lagu acak dari library (lagu nyata diutamakan), cincin berdenyut mengikuti
  beat lagu yang sebenarnya (via TimingMap), label "♪ judul — artis".
- Perbaikan: `Music::fade_out` kini tidak terpotong saat objek di-drop (preview Song Select ikut lebih halus).

### Fase 7 — Kualitas & kompatibilitas (selesai sebagian; sisanya butuh hardware pengguna)
- VSync (`present_mode` AutoVsync/AutoNoVsync) dan backend grafis (Otomatis/Vulkan/DX12/OpenGL) dari Settings
  diterapkan saat start. Variabel `WGPU_BACKEND` tetap dihormati pada mode Otomatis.
- Bila jendela grafis gagal dibuka: dialog ramah (rfd) menjelaskan masalah driver dan menawarkan mode OpenGL
  untuk start berikutnya (tidak bisa retry dalam proses yang sama: winit hanya boleh 1 event loop). Teruji.
- Panic → `crash.log` di folder config (untuk dilampirkan saat lapor bug).
- Audit `unwrap/expect`: hanya 1 `expect` invarian internal di luar test.
- Pengukuran (build release, Intel Iris Xe, layar judul + musik):
  - Wayland (KWin): ~60 FPS di semua backend walau VSync mati → dibatasi frame callback compositor/monitor 60 Hz.
  - X11 (XWayland, VSync mati): 117–131 FPS.
  - Waktu CPU bangun frame: 0,2–0,9 ms (anggaran 120 FPS = 8,3 ms) → ruang performa sangat besar.
  - Log: `RUST_LOG=ferrostep=debug`.
- Teruji jalan: Wayland (Auto/Vulkan/OpenGL) dan X11.

**Belum bisa diuji di sini (butuh pengguna):** Windows 11 (DX12/Vulkan, joy.cpl), GNOME, dance mat sungguhan
(latensi polling, debounce, hot-plug), ukur FPS gameplay di monitor 120 Hz+.
