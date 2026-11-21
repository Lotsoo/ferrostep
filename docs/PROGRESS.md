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

### Tambahan — bar skor (permintaan pengguna)
- Di bawah bar progres lagu: bar skor (terang = poin didapat, redup = persentase maksimum yang masih mungkin),
  penanda grade C/B/A/S/SS yang meredup bila sudah tidak terjangkau. `Score::max_possible_percent` + test.

### Tambahan — HUD gameplay (permintaan pengguna)
- **Life bar** horizontal bersegmen di kiri atas (merah → kuning → hijau, berdenyut merah saat < 25 %,
  label "No Fail"). Menggantikan bar vertikal tipis.
- Angka kanan atas = **akurasi saat ini** (mulai 100 %, gaya Etterna); sama dengan skor final di akhir lagu.
- **Bar skor** di bawahnya: poin terkumpul + area "masih bisa dicapai" + penanda B/A/S/SS.
- Alat developer: `--play <folder>`, `--chart <n>`, `--autoplay` (`GameSession::autoplay`, teruji: tap, hold,
  roll, mine → 100 %). AutoPlay tidak menyimpan skor dan tidak dijeda saat jendela kehilangan fokus.
- Diverifikasi lewat screenshot jendela (X11) saat autoplay lagu impor 4 menit.

### Fase 8 — Packaging & rilis (selesai; RPM dibuat oleh pengguna/CI)
- Versi 0.1.0, lisensi **MIT** ("Ferrostep contributors") — *perlu dikonfirmasi pemilik*.
- Ikon orisinal (SVG + PNG 32–256), ikon jendela, `packaging/ferrostep.desktop` (lolos desktop-file-validate).
- `[package.metadata.generate-rpm]` (format diverifikasi dari README cargo-generate-rpm 0.21.1).
- Profil release: thin LTO, strip debuginfo (nama simbol tetap untuk crash.log). Biner ±35 MB, aset tertanam.
- `.github/workflows/build.yml`: Fedora 45 container (fmt, clippy, test, RPM) + Windows (clippy, test, zip),
  release otomatis saat push tag `v*`. Belum dijalankan (repo belum punya remote GitHub).
- README untuk pengguna akhir (instal, main, panduan beli & pasang dance mat, masalah umum, build).
- `docs/ARCHITECTURE.md`.

### Redesign — prinsip desain Apple (permintaan pengguna, skill `apple-design`)

- `anim::Spring`: pegas yang bisa disela (damping + response), plus `project` (proyeksi momentum) dan `rubberband` (tepi lentur), dengan test.
- Transisi layar memakai pegas dan arah navigasi: layar lebih dalam masuk dari kanan, kembali masuk dari kiri. "Kurangi animasi" = cross-fade 150 ms saja.
- Daftar lagu: scroll berbasis pegas (roda mouse/tombol), drag 1:1 dengan tepi lentur dan lemparan bermomentum, satu sorotan fokus yang meluncur antar kartu, tepi daftar memudar.
- Tombol/kartu langsung bereaksi saat ditekan (mengecil 3 %), bayangan lembut bertingkat, garis terang tipis di tepi atas.
- Tipografi: letter-spacing menyesuaikan ukuran (judul besar dirapatkan, label kapital kecil direnggangkan).
- Opsi developer `--open songs|import|settings` untuk memotret layar.
- Perbaikan kecil: subjudul gameplay tidak lagi diawali "·" bila artis kosong.
- Perlu dirasakan pengguna: kenyamanan scroll/drag di Song Select (nilai pegas mudah disetel di `anim.rs`).

### Redesign gameplay + kursor (permintaan pengguna: "terlalu sepi dan jadul")

- Modul baru `ui/hud.rs`: panggung (cahaya radial halus, partikel melayang, vignette), lajur kaca dengan garis ketukan yang ikut bergulir dan sorot cahaya kolom saat diinjak.
- "Energi combo": combo ≥ 10 membuat panggung, cahaya, dan tepi lajur makin hidup; turun lembut saat combo putus.
- HUD: pulau kaca info lagu (equalizer kecil, progres, sisa waktu) dan pulau skor (grade live, akurasi beranimasi, bar skor, jumlah tiap judgment). Life bar segmen lama diganti tabung vertikal di samping lajur dengan ikon hati; peringatan berdenyut pelan (1,5 Hz).
- Judgment dengan cahaya lembut + kilau Marvelous; petunjuk CEPAT/LAMBAT (ms) untuk Great ke bawah; meter ketepatan 24 tekanan terakhir; combo emas selama full combo; perayaan + pujian setiap 50 combo; hitung mundur berbentuk cincin + "Mulai!".
- `notefield::radial_light`: semua glow memakai mesh radial (tanpa cincin/banding).
- Kursor tangan saat menyorot apa pun yang bisa diklik (global di `app.rs`), kursor "grabbing" saat menyeret daftar lagu.

### Navigasi, kontras grade, impor banyak file (permintaan pengguna)

- Tombol "‹ Kembali" di Pilih Lagu; tombol "Jeda" di gameplay untuk pengguna mouse (Esc tetap berfungsi).
- Badge grade kontras tinggi: tile hampir hitam, cincin + huruf berwarna terang, outline gelap di belakang huruf.
- Impor banyak file: sebelumnya antrean berhenti di kartu hasil lagu pertama (terlihat seperti hanya 1 lagu terbaca). Sekarang berjalan otomatis ("Lagu 2 dari 8"), file gagal tidak menghentikan antrean, dan diakhiri kartu ringkasan (berhasil/gagal + alasan ramah). Satu file tetap memakai kartu hasil detail (koreksi BPM).
- File audio sebagai argumen langsung diimpor (`ferrostep a.mp3 b.mp4`), siap untuk "Buka dengan Ferrostep".

## Berikutnya / masalah terbuka
- Pengguna: uji di Windows 11 (zip dari CI atau `cargo build --release`), uji dance mat sungguhan
  (evtest → wizard → main), uji GNOME, uji FPS gameplay di monitor 120 Hz+.
- Konfirmasi lisensi (MIT?) dan nama pemegang hak cipta.
- Ide yt-dlp/streaming YouTube: **ditolak pengguna** (2026-11-21). Jangan dikerjakan kecuali diminta lagi.
- Fase lanjut: BPM bervariasi, video latar MP4, editor chart, mode latihan, statistik riwayat,
  ikon .exe Windows (winres), AppImage.
- Drag-and-drop di Wayland menunggu dukungan winit.

## Sesi 2026-11-21 (lanjutan, belum di-commit)
- Selesai: hold-to-repeat menu (keyboard & mat), efek suara arcade baru (stereo, reverb, saturasi; volume efek tidak ikut volume musik), hitung mundur 5 detik (kartu BERSIAP → 3-2-1 dengan flash/shockwave + suara → MULAI!), dialog konfirmasi keluar, badge grade berwarna, petunjuk tombol tebal, impor duplikat dilewati (hash, byte, fingerprint chart).
- Berikutnya: pengguna menilai suara baru; update ARCHITECTURE/README; commit batch ini; screenshot dialog keluar.

## Sesi 2026-11-21 — redesign "rasa rhythm game" (tetap Rust/egui)
- Pengguna memilih tetap di egui (bukan Tauri + React) setelah penjelasan soal FPS WebKitGTK & biaya tulis ulang.
- Modul baru `ui/beatfx.rs`: `Beat` (pulse/downbeat dari timing lagu), `draw_highway` (lantai neon perspektif, satu garis per ketukan), `draw_equalizer` (bar ala spektrum, mengikuti ketukan), `draw_metronome`, `draw_kick_ring`.
- Pilih Lagu: highway, equalizer di banner, metronom di BPM, kartu terpilih berdenyut — sinkron dengan preview.
- Judul: logo yang hurufnya melompat bergelombang, 4 panah menyala bergiliran, equalizer, bingkai menu berdenyut, label "sedang diputar".
- Hasil: drum roll → grade menghantam (kilatan + gelombang kejut + suara) → stempel FULL COMBO / REKOR BARU!; angka judgment menghitung naik.
- Gameplay: highway + equalizer hanya di sisi kiri/kanan jalur, makin hidup seiring combo.
- Suara atas/bawah/kesulitan: satu nada pendek dengan "instrumen" yang sama seperti Enter/Esc (yang disukai pengguna).
- Berikutnya: minta pendapat pengguna; ide lanjutan: transisi layar "swoosh" berirama, efek hit yang lebih besar di combo milestone.

## Sesi 2026-11-21 — redesign ritme bagian 2 (semua layar)
- Impor: zona drop dengan panah jatuh bergiliran + bingkai neon berdenyut; garis pemindai di waveform; setelah selesai lagu langsung diputar sebagai preview, angka BPM + metronom + equalizer berdenyut tepat mengikuti BPM terdeteksi (preview diulang otomatis saat ÷2/×2).
- Jeda: kartu baru (garis 4 warna, progres lagu, akurasi & combo live) + hitung mundur 3-2-1 sebelum lagu dilanjutkan (jeda dari mana pun membatalkan hitungan).
- Pengaturan: warna & ikon panah per kategori, lantai neon kalem. Kalibrasi: strip "cepat ← → lambat" dengan penilaian tiap ketukan (umpan balik setelah tekan, bukan isyarat — supaya hasil kalibrasi tidak bias).
- Wizard dance mat: not jatuh ke panah yang diminta, ledakan cahaya + suara saat tertangkap/diinjak, lantai berwarna sesuai panah.
- Onboarding: indikator langkah berupa jalur not, panah menyala bergiliran.
- Gameplay: gelombang 4 warna + percikan tiap combo kelipatan 50, suara di kelipatan 100.
- Transisi layar: 4 garis cahaya warna kolom melesat searah perpindahan (durasi tetap, tanpa menunggu ketukan).
- Catatan teknis: XTest tidak sampai ke jendela XWayland di KDE, jadi uji visual jeda/kalibrasi/wizard memakai patch sementara yang tidak di-commit.
- Pengaturan: slider buatan sendiri (kotak angka tebal, jalur gradien, kenop besar), ID pilihan bersegmen unik (peringatan merah egui di build debug hilang), path panjang dibungkus, teks sidebar dipendekkan dengan "…".
- Hapus lagu: tombol "Hapus" / tombol Del di Pilih Lagu → dialog konfirmasi (Batal default) → folder lagu dihapus permanen. Pengaman: hanya folder lagu tunggal di dalam Songs/ yang berisi .sm/.ssc; folder grup kosong ikut dibersihkan. Penghapusan ditunda ±0,35 s setelah preview dihentikan (Windows mengunci file yang sedang diputar — belum diuji di Windows).
