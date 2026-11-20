# CLAUDE.md — Proyek "Ferrostep": Rhythm Game Desktop Modern (Rust)

> Nama proyek: **Ferrostep** (sama dengan nama folder repo). Nama crate & binary: `ferrostep`.
> Lingkungan development pemilik: **Fedora 45, desktop KDE Plasma, sesi Wayland**. Target rilis: Windows 11 dan Fedora 45.
> Dokumen ini adalah instruksi utama untuk Claude Code. Baca seluruhnya sebelum menulis kode apa pun.

---

## 0. PENTING: Siapa penggunanya dan bagaimana cara memandunya

Pemilik proyek ini **masih awam dengan Rust** dan belum paham arsitektur aplikasi Rust. Ia punya ide yang jelas, tetapi butuh pemandu. Karena itu kamu (Claude Code) wajib bertindak sebagai **mentor sekaligus engineer**:

1. **Tanyakan dulu di awal sesi pertama:** sistem operasi yang sedang dipakai (Windows 11 atau Fedora 45), apakah Rust sudah terpasang (`rustc --version`, `cargo --version`), dan apakah Git sudah terpasang. Jangan berasumsi.
2. **Kerjakan dalam langkah kecil.** Satu fase (lihat §9) dipecah menjadi langkah yang masing-masing bisa dikompilasi dan dijalankan. Jangan menulis ribuan baris sekaligus.
3. **Setiap selesai satu langkah, jelaskan dalam bahasa Indonesia yang sederhana:**
   - apa yang baru dibuat dan kenapa,
   - perintah persis yang harus diketik pengguna (beri versi PowerShell untuk Windows dan bash untuk Fedora jika berbeda),
   - apa yang seharusnya terlihat jika berhasil.
4. **Ajarkan konsep Rust saat pertama kali muncul**, singkat (2–4 kalimat) dengan analogi: *ownership & borrowing*, `Result`/`Option` dan operator `?`, `struct`/`enum`/`match`, *trait*, modul (`mod`, `pub`), *thread* dan *channel*, `Arc`/`Mutex`/atomic. Jangan menggurui; cukup saat relevan.
5. **Saat ada error kompilasi**, tunjukkan baris error yang penting, jelaskan artinya dalam bahasa awam, lalu perbaiki. Jangan diam-diam menekan warning dengan `#[allow(...)]` tanpa alasan.
6. **Jangan mengarang.** Jika tidak yakin dengan versi crate, nama paket `dnf`, atau API sebuah crate, cek dulu (`cargo add`, `cargo search`, dokumentasi di docs.rs, `dnf search`) dan katakan terus terang bila belum terverifikasi.
7. **Minta konfirmasi sebelum** keputusan besar yang sulit dibatalkan: mengganti framework, menghapus file, mengubah format data yang sudah tersimpan, atau memasang tool sistem.
   - **Pengguna sendiri yang menginstal semua tool sistem** (rustup, `sudo dnf install`, `cargo install`, dll.). Claude Code tidak menjalankan perintah instalasi; cukup berikan perintah persis, jelaskan gunanya, lalu verifikasi hasilnya (mis. `rustc --version`) setelah pengguna selesai. Verifikasi nama paket dengan `dnf repoquery` sebelum memberikannya.
   - Shell login pengguna adalah **fish**; beri catatan bila perintah berbeda antara fish dan bash.
8. Di akhir setiap sesi, tulis ringkasan kemajuan ke `docs/PROGRESS.md` (apa yang selesai, apa berikutnya, masalah terbuka) supaya sesi berikutnya bisa melanjutkan.
9. Bahasa komunikasi dengan pengguna: **bahasa Indonesia**. Bahasa kode, nama variabel, dan komentar kode: **bahasa Inggris** (standar ekosistem Rust).

---

## 1. Visi produk

Aplikasi desktop **rhythm game 4-panel** (mirip StepMania/Etterna) yang:

- **Memainkan simfile** StepMania (`.sm` dan `.ssc` + audio) dari folder `Songs/`.
- Punya **importer bawaan MP3/MP4 → simfile**: pengguna memilih atau *drag-and-drop* file `.mp3`/`.mp4`, aplikasi mengekstrak audio, mendeteksi BPM/offset/onset, membuat chart otomatis (Beginner/Easy/Medium/Hard), menyimpan ke `Songs/Imported/<Judul>/`, lalu lagu langsung muncul di daftar dan bisa dimainkan.
- **Tampilan modern, minimalis, ramah pengguna, dan memukau.** StepMania terasa jadul; Ferrostep harus terasa seperti aplikasi tahun 2026 (lihat §6). Ini **prioritas setara** dengan fungsionalitas, bukan tambahan belakangan.
- Berjalan **native** (tanpa Docker, tanpa web server) di **Windows 11 x86_64** dan **Fedora 45 x86_64** (Wayland/X11, PipeWire; diuji di KDE Plasma dan GNOME).
- **Tujuan akhir: dimainkan dengan dance mat USB yang dibeli dari e-commerce, secara *plug and play*.** Colok mat → aplikasi langsung mengenalinya (notifikasi "Dance mat terdeteksi") → bila tombolnya belum dikenal, wizard singkat "injak panah kiri… bawah… atas… kanan" → langsung main. Tanpa driver tambahan, tanpa file konfigurasi manual, tanpa terminal. Keyboard tetap didukung penuh. Detail di §5.9.
- **Mudah dipakai orang awam:** unduh, ekstrak/instal, klik dua kali, main. Lihat §6.4.

---

## 2. Batasan keras

- Satu binary, satu proses. Tidak ada aplikasi web terpisah.
- File yang dihasilkan importer harus **valid dan bisa dibuka di StepMania asli** (format `.sm` standar).
- Jangan menyertakan lagu, gambar, atau aset berhak cipta di repo. Aset visual harus dibuat sendiri (bentuk vektor yang digambar kode) atau berlisensi bebas (mis. font Inter, SIL OFL). Untuk pengujian, **buat audio sintetis lewat kode** (lihat §8).
- Jangan meniru aset atau logo DDR/StepMania/Etterna. Tampilan harus orisinal.
- Gameplay harus disinkronkan ke **jam audio**, bukan sekadar akumulasi *delta time* frame.

---

## 3. Keputusan teknologi

Pilihan default di bawah sengaja dibuat **sesederhana mungkin untuk pemula** tanpa mengorbankan performa. Pada Fase 0, verifikasi bahwa versi terbaru crate-crate ini saling kompatibel. Pakai `cargo add <crate>` agar versi diambil otomatis; jangan menulis nomor versi dari ingatan.

| Kebutuhan | Pilihan default | Alasan |
|---|---|---|
| Window + UI + rendering | **`eframe` (egui) dengan backend `wgpu`** | Satu framework untuk menu dan gameplay; *immediate mode* mudah dipahami pemula; `wgpu` memakai DX12/Vulkan di Windows dan Vulkan di Linux; mendukung Wayland & X11. Gameplay digambar dengan `egui::Painter` (shape vektor) sehingga tidak perlu engine game penuh. |
| Audio playback | **`kira`** | Latensi rendah, berbasis `cpal` (ALSA di Linux → dilayani PipeWire lewat lapisan ALSA-nya; WASAPI di Windows), menyediakan posisi playback untuk jam audio. |
| Decode audio | **`symphonia`** (fitur: mp3, aac, isomp4, ogg/vorbis, wav, flac) | Pure Rust; bisa membaca audio AAC langsung dari `.mp4` tanpa ffmpeg. |
| FFT | **`rustfft`** | Standar de facto. |
| Tulis WAV fallback | **`hound`** | Jika ffmpeg tidak ada. |
| Ekstraksi/transcode audio | **`ffmpeg` CLI** via `std::process::Command` (opsional) | Untuk menghasilkan `.ogg` yang ringkas. Tidak wajib: fallback ke WAV atau copy MP3. |
| File dialog | **`rfd`** | Native di kedua OS (pada Linux memakai XDG portal). |
| Pantau folder (hot-reload) | **`notify`** | Mendeteksi lagu baru di `Songs/`. |
| Thread & channel | `std::thread` + **`crossbeam-channel`** | Sederhana dan aman. |
| Konfigurasi & skor | **`serde`** + **`toml`** (config) + **`serde_json`** (skor) | Mudah dibaca manusia. |
| Lokasi folder config | **`directories`** | Path yang benar per OS. |
| Error handling | **`anyhow`** (aplikasi) + **`thiserror`** (modul library) | Pola umum Rust. |
| Logging | **`log`** + **`env_logger`** | Debug mudah: `RUST_LOG=debug`. |
| Gamepad / dance mat (**fitur inti**, mulai Fase 3) | **`gilrs`** | Kebanyakan dance mat USB terbaca sebagai gamepad HID generik (evdev di Linux, Windows.Gaming.Input/XInput di Windows). Mendukung hot-plug dan memberi kode tombol mentah untuk perangkat yang belum dikenal. Verifikasi dukungan perangkat DirectInput-only di Windows saat Fase 3. |
| Angka acak deterministik | **`rand`** + **`rand_chacha`** | Chart yang sama untuk lagu yang sama (seed dari hash audio). |

Jika pada Fase 0 ditemukan masalah serius (mis. `kira` tidak memberi posisi playback yang cukup presisi), jelaskan ke pengguna, usulkan alternatif (mis. mixer sendiri di atas `cpal` yang menghitung sampel terputar), dan minta persetujuan sebelum mengganti.

---

## 4. Arsitektur & struktur direktori

Satu crate aplikasi dengan modul-modul terpisah. Logika inti (parser, DSP, chart_gen, timing, judgment) **tidak boleh bergantung pada UI**, supaya bisa diuji dengan `cargo test` tanpa membuka jendela.

```
ferrostep/
├── Cargo.toml
├── CLAUDE.md
├── README.md
├── assets/
│   └── fonts/                 # Inter (OFL) + file LICENSE-nya
├── docs/
│   ├── PROGRESS.md            # log kemajuan antar sesi
│   └── ARCHITECTURE.md        # diperbarui saat arsitektur berubah
├── Songs/                     # di-gitignore kecuali contoh sintetis
├── src/
│   ├── main.rs                # entry point: setup logging, config, jalankan eframe
│   ├── app.rs                 # state machine layar (Screen enum) + routing event
│   ├── config.rs              # Settings: keybind, global offset, scroll speed, volume, tema
│   ├── library/               # pemindaian Songs/, cache metadata, hot-reload (notify)
│   │   ├── mod.rs
│   │   └── scanner.rs
│   ├── parser/                # baca .sm/.ssc, tulis .sm
│   │   ├── mod.rs             # tipe data Simfile, Chart, Note, TimingData
│   │   ├── sm.rs
│   │   ├── ssc.rs
│   │   └── writer.rs
│   ├── timing/                # konversi beat <-> detik (BPMS, STOPS, OFFSET)
│   │   └── mod.rs
│   ├── audio/                 # playback (kira) + AudioClock
│   │   ├── mod.rs
│   │   └── clock.rs
│   ├── audio_dsp/             # decode, resample/mono, STFT, onset, BPM, offset
│   │   ├── mod.rs
│   │   ├── decode.rs
│   │   ├── onset.rs
│   │   ├── tempo.rs
│   │   └── synth.rs           # generator audio sintetis untuk test
│   ├── chart_gen/             # onset + tempo -> chart 4-panel per difficulty
│   │   ├── mod.rs
│   │   ├── quantize.rs
│   │   └── patterns.rs        # aturan ergonomis
│   ├── importer/              # pipeline MP3/MP4 -> folder lagu, di background thread
│   │   ├── mod.rs
│   │   └── ffmpeg.rs          # pencarian binary & pemanggilan CLI
│   ├── input/                 # sumber input terpadu: keyboard + gamepad/dance mat (gilrs)
│   │   ├── mod.rs             # InputEvent { column, pressed, time } dari semua perangkat
│   │   ├── gamepad.rs         # hot-plug, profil perangkat, debounce
│   │   └── profiles.rs        # pemetaan tombol per perangkat (disimpan per vendor/product ID)
│   ├── gameplay/              # state permainan, judgment, skor, combo
│   │   ├── mod.rs
│   │   ├── judge.rs
│   │   └── score.rs
│   └── ui/                    # semua tampilan
│       ├── mod.rs
│       ├── theme.rs           # design tokens (§6)
│       ├── widgets.rs         # tombol, kartu, progress bar custom
│       ├── anim.rs            # easing & tween
│       ├── screens/
│       │   ├── title.rs
│       │   ├── song_select.rs
│       │   ├── import.rs
│       │   ├── gameplay.rs
│       │   ├── results.rs
│       │   └── settings.rs    # termasuk wizard kalibrasi offset
│       └── notefield.rs       # gambar receptor, panah, hold, efek hit
└── tests/
    ├── parser_roundtrip.rs
    ├── dsp_synthetic.rs
    └── fixtures/              # simfile kecil buatan sendiri
```

### Alur data utama

```
[Songs/] --scanner--> [Library cache] --> Song Select UI
                                              │ pilih lagu + difficulty
                                              ▼
[parser] -> Simfile -> [timing] -> daftar note (waktu dalam detik)
                                              │
[audio] mulai playback -> AudioClock ---------┤
                                              ▼
         [gameplay] (input + judge + score) <-> [ui/notefield] render tiap frame
                                              ▼
                                        Results Screen -> simpan skor

[Import UI] --(path file)--> importer thread:
   decode (symphonia / ffmpeg) -> audio_dsp (onset, BPM, offset)
   -> chart_gen (4 difficulty) -> parser::writer (.sm) -> Songs/Imported/<Judul>/
   --(progress via channel)--> UI progress bar
   --(selesai)--> library refresh (hot-reload)
```

### State machine layar

```rust
enum Screen { Title, SongSelect, Import, Gameplay(GameSession), Results(ResultData), Settings }
```

Transisi antar layar wajib memakai animasi (fade/slide 200–300 ms).

---

## 5. Spesifikasi per modul

### 5.1 `parser`

**Membaca `.sm`:** tag berformat `#TAG:nilai;`. Wajib didukung: `#TITLE`, `#SUBTITLE`, `#ARTIST`, `#BANNER`, `#BACKGROUND`, `#MUSIC`, `#OFFSET`, `#SAMPLESTART`, `#SAMPLELENGTH`, `#BPMS`, `#STOPS`, `#NOTES`.

- Abaikan komentar `//` sampai akhir baris. Toleran terhadap whitespace, CRLF/LF, BOM UTF-8, dan tag yang tidak dikenal (simpan saja, jangan crash).
- `#BPMS` dan `#STOPS`: daftar `beat=nilai` dipisah koma.
- `#NOTES` berisi 6 field dipisah `:`: `steps_type : description : difficulty : meter : radar_values : note_data`. MVP hanya memakai `dance-single` (4 kolom: Left, Down, Up, Right); tipe lain disimpan tetapi tidak ditampilkan.
- `note_data`: measure dipisah `,`. Setiap measure berisi N baris (N = 4, 8, 12, 16, 24, 32, 48, 64, 192, dst.), satu baris = 4 karakter. Beat sebuah baris = `measure_index * 4 + (row_index / N) * 4`.
- Karakter note: `0` kosong, `1` tap, `2` kepala hold, `3` ekor hold/roll, `4` kepala roll, `M` mine, `L` lift, `F` fake. MVP: tap, hold, roll, mine wajib; `L`/`F` boleh diperlakukan sebagai tap/diabaikan dengan log warning.
- Konvensi offset: **waktu (detik) dari beat 0 = −`#OFFSET`**. Tulis unit test untuk ini karena sering terbalik.

**Membaca `.ssc`:** header lagu sama, lalu tiap chart diawali `#NOTEDATA:;` diikuti `#STEPSTYPE`, `#DIFFICULTY`, `#METER`, `#NOTES`, dan bisa punya `#BPMS`/`#OFFSET` per chart (jika ada, timing per chart mengalahkan timing lagu). Jika sebuah lagu punya `.sm` dan `.ssc`, utamakan `.ssc`.

**Menulis `.sm`:** importer menghasilkan `.sm` (format paling kompatibel). Kuantisasi tiap measure ke resolusi terkecil yang cukup (4/8/16/24/48/192 baris). Wajib ada **test round-trip**: parse → tulis → parse lagi, hasilnya identik.

**Tipe data inti (usulan):**
```rust
pub struct Simfile { pub meta: Metadata, pub timing: TimingData, pub charts: Vec<Chart> }
pub struct TimingData { pub offset: f64, pub bpms: Vec<(f64, f64)>, pub stops: Vec<(f64, f64)> }
pub struct Chart { pub steps_type: String, pub difficulty: Difficulty, pub meter: u32, pub notes: Vec<Note>, pub timing_override: Option<TimingData> }
pub struct Note { pub beat: f64, pub column: u8, pub kind: NoteKind }
pub enum NoteKind { Tap, HoldHead { end_beat: f64 }, RollHead { end_beat: f64 }, Mine, Lift, Fake }
```

### 5.2 `timing`

- `beat_to_seconds(beat)` dan `seconds_to_beat(sec)` yang mendukung banyak segmen BPM dan STOPS. Bangun tabel segmen sekali, lalu cari dengan *binary search*.
- Semua waktu note dihitung **sekali** saat lagu dimuat menjadi `Vec<TimedNote { time: f64, column, kind, ... }>` terurut.
- Test: BPM konstan, perubahan BPM, stop, offset positif/negatif.

### 5.3 `audio` + `AudioClock` (inti sinkronisasi)

Masalah: posisi playback dari backend audio diperbarui per *buffer* (mis. tiap 5–20 ms), sedangkan layar 120+ FPS. Jika langsung dipakai, panah tampak tersendat.

Solusi wajib:
1. Setiap frame, baca `raw = posisi playback dari kira`.
2. Simpan `(last_raw, last_instant)`. Jika `raw` berubah, set `last_raw = raw`, `last_instant = Instant::now()`.
3. Waktu halus: `smooth = last_raw + (now - last_instant)` (dikali playback rate), lalu **dibatasi** agar tidak melampaui `last_raw + toleransi` (≈ 1 buffer) dan **tidak pernah mundur** dari nilai frame sebelumnya.
4. `song_time = smooth − global_offset` (global offset dari config, satuan detik, bisa dikalibrasi).
5. Input keyboard dinilai memakai `song_time` saat event diterima. Kumpulkan event dari egui di awal frame.
6. Sebelum lagu mulai, sediakan *lead-in* (mis. 2 detik hitung mundur) memakai clock monoton, lalu serahkan ke audio clock tanpa lompatan.

Volume, pause/resume, dan *preview* lagu di Song Select (mulai dari `#SAMPLESTART`, fade in/out) juga ada di modul ini.

### 5.4 `audio_dsp` (analisis)

1. **Decode** dengan symphonia → mixdown mono `f32` → resample ke 44 100 Hz (atau tetap di sample rate asli; konsisten saja). Untuk `.mp4`, symphonia membaca track audio AAC; jika gagal, fallback ke ffmpeg.
2. **STFT:** window Hann 2048, hop 512 (≈ 11,6 ms pada 44,1 kHz).
3. **Onset detection function:** *spectral flux* pada magnitudo log (`ln(1 + γ·|X|)`), hanya selisih positif, dijumlahkan per frame. Opsional: pisahkan pita frekuensi rendah (kick) dan tinggi (snare/hi-hat) untuk dipakai chart_gen.
4. **Peak picking:** ambang adaptif (median bergerak + delta), jarak minimum antar onset ≈ 50 ms. Simpan kekuatan tiap onset.
5. **BPM:** autokorelasi fungsi onset pada rentang lag untuk 70–200 BPM, diberi bobot preferensi di sekitar 120 BPM untuk mengurangi kesalahan *double/half tempo*. Haluskan hasil ke presisi 0,01 BPM dengan interpolasi parabola puncak. Jika kandidat mendekati bilangan bulat (selisih < 0,05), bulatkan.
6. **Offset (first beat):** cari fase grid beat yang memaksimalkan jumlah kekuatan onset di titik grid (uji fase 0 sampai 1 periode dengan resolusi 1 ms). Konversi ke konvensi `#OFFSET` (§5.1).
7. Asumsi MVP: **BPM konstan**. Lagu dengan tempo berubah dicatat sebagai keterbatasan; dukungan BPM bervariasi masuk fase lanjut.
8. Semua fungsi menerima `&[f32]` dan sample rate, mengembalikan struct hasil. **Tanpa** I/O di dalamnya, supaya mudah diuji.
9. Laporkan progres (0–100%) lewat callback/closure agar UI bisa menampilkan progress bar.

### 5.5 `chart_gen`

Input: onset (waktu + kekuatan + pita), BPM, offset, durasi. Output: `Chart` untuk Beginner, Easy, Medium, Hard.

- **Kuantisasi** onset ke grid terdekat; resolusi per difficulty: Beginner 1/4 (hanya beat), Easy 1/8, Medium 1/8 (+ sedikit 1/16 pada onset kuat), Hard 1/16.
- **Seleksi densitas:** urutkan onset menurut kekuatan; ambil sebanyak target NPS (*notes per second*) per difficulty (mis. kira-kira Beginner ≤ 1,5, Easy ≤ 2,5, Medium ≤ 4,5, Hard ≤ 7; jadikan konstanta yang mudah disetel).
- **Aturan ergonomis (wajib):**
  - Batasi *jack* (kolom sama berurutan): tidak boleh lebih dari 2 kali berturut-turut untuk Beginner–Medium, maksimal 3 untuk Hard, dan tidak boleh jack jika jarak antar note < 1/8 beat pada BPM tinggi.
  - Pola kaki bergantian kiri/kanan (Left = kaki kiri, Right = kaki kanan, Up/Down bebas). Hindari *crossover* di Beginner/Easy.
  - *Jump* (2 panah bersamaan) hanya pada onset sangat kuat yang jatuh tepat di beat; tidak ada jump di Beginner; jarang di Easy.
  - Hold untuk nada panjang yang stabil (energi bertahan) hanya di Medium/Hard; jangan ada note lain di kolom yang sedang di-hold.
  - Tidak ada note dalam 1 detik pertama setelah audio mulai bila memungkinkan.
- **Deterministik:** RNG di-seed dari hash isi audio, sehingga import ulang lagu yang sama menghasilkan chart yang sama.
- **Meter** (angka kesulitan 1–10) diperkirakan dari NPS rata-rata dan puncak.
- Unit test: tidak ada pelanggaran aturan ergonomis pada chart hasil generate dari onset acak.

### 5.6 `importer`

- Berjalan di **background thread**. UI tidak boleh freeze. Komunikasi lewat channel: `ImportEvent::{Progress(stage, pct), Done(path), Failed(error)}`. Dukung **batal** lewat `Arc<AtomicBool>`.
- Tahapan yang ditampilkan ke pengguna: *Membaca file → Mengekstrak audio → Menganalisis ketukan → Membuat chart → Menyimpan*.
- **Pencarian ffmpeg:** (1) path di config, (2) folder yang sama dengan executable, (3) `PATH` (`ffmpeg.exe` di Windows, `ffmpeg` di Linux). Jika tidak ditemukan, tetap jalan dengan symphonia + fallback; tampilkan info ramah (bukan error menakutkan) bahwa memasang ffmpeg menghasilkan file lebih kecil.
- Output audio: MP3 input → disalin apa adanya. MP4 input → `.ogg` via ffmpeg jika ada; jika tidak, `.wav` via hound.
- Judul lagu: dari tag metadata (symphonia) jika ada, kalau tidak dari nama file. Sanitasi nama folder (hapus karakter ilegal Windows `<>:"/\|?*`, trim, batasi panjang). Jika folder sudah ada, tambahkan sufiks ` (2)`.
- Tulis `#TITLE`, `#ARTIST`, `#MUSIC`, `#OFFSET`, `#BPMS`, `#SAMPLESTART` (bagian paling energik), `#SAMPLELENGTH`, dan 4 chart. Tambahkan `#CREDIT:Ferrostep Auto-Chart;`.
- Opsional fase lanjut: salin `.mp4` sebagai video latar.
- *Drag-and-drop*: tangani `ctx.input(|i| i.raw.dropped_files)` di layar mana pun → buka layar Import dengan file tersebut. Tampilkan overlay "Lepaskan untuk mengimpor" saat file sedang di-hover.

### 5.7 `library` + hot-reload

- Pindai `Songs/` (rekursif dua tingkat: `Songs/<Grup>/<Lagu>/`). Kumpulkan metadata ringan saja di awal; parse penuh saat lagu dipilih.
- Pemindaian di background thread; tampilkan skeleton loading di UI.
- `notify` memantau `Songs/`; perubahan di-*debounce* (≈ 500 ms) lalu memindai ulang folder yang berubah. Importer juga mengirim event langsung agar lagu baru muncul seketika dengan animasi highlight.
- Lokasi `Songs/`: default di samping executable; jika tidak bisa ditulis, pakai folder data pengguna (`directories`). Bisa diubah di Settings.

### 5.8 `gameplay`

- **Kolom:** Left, Down, Up, Right. Keybind default `D F J K` dan tombol panah; bisa diubah di Settings (layar "tekan tombol untuk mengikat").
- **Arah scroll:** panah bergerak dari bawah ke atas menuju receptor di atas (default), opsi *reverse*.
- **Kecepatan:** gunakan *constant scroll* (mis. "XMod" dan "CMod"); default CMod 500 agar mudah dibaca pemula.
- **Jendela judgment (default, gaya StepMania), selisih absolut waktu tekan vs waktu note:**
  - Marvelous ≤ 22,5 ms
  - Perfect ≤ 45 ms
  - Great ≤ 90 ms
  - Good ≤ 135 ms
  - Boo ≤ 180 ms
  - Miss: tidak ditekan sampai lewat 180 ms
  Simpan sebagai konstanta yang mudah diubah, dengan pengali "Judge difficulty".
- Satu tekanan hanya menilai note terdekat yang belum dinilai di kolom itu.
- Hold: harus ditahan sampai ekor (toleransi lepas ≈ 250 ms); hasil *OK/NG*. Mine: kena jika tombol ditahan saat mine lewat.
- Combo naik pada Marvelous/Perfect/Great; putus pada Good ke bawah (perilaku StepMania).
- Skor: persentase gaya Etterna/DP sederhana. Simpan juga histogram selisih waktu (mean dan standar deviasi) untuk layar hasil dan kalibrasi.
- Life bar sederhana; MVP boleh tanpa *fail* (opsi "No Fail" default aktif untuk pemula).
- Gameplay hanya menerima `InputEvent` dari modul `input` (§5.9), tidak peduli asalnya keyboard atau dance mat.

### 5.9 `input` — keyboard & dance mat *plug and play*

Tujuan: pengguna membeli dance mat USB murah dari e-commerce, mencolokkannya, dan langsung bisa main di Windows 11 maupun Fedora 45.

- **Hot-plug:** deteksi perangkat dicolok/dicabut kapan saja (event `gilrs` Connected/Disconnected) dan tampilkan *toast*: "Dance mat terhubung: <nama>" / "Dance mat terputus — game dijeda". Jika terputus saat gameplay, otomatis pause.
- **Profil otomatis:** simpan pemetaan tombol per perangkat (kunci: vendor ID + product ID + nama) di folder config. Mat yang pernah diatur langsung dikenali lagi.
- **Wizard pemetaan pertama kali:** jika perangkat belum punya profil, tampilkan layar besar bergambar: "Injak panah KIRI" → bawah → atas → kanan, lalu opsional "Start/Back". Pakai kode tombol/axis **mentah** sehingga mat yang memetakan panah sebagai tombol, D-pad, *hat*, ataupun axis analog tetap bisa dipakai. Bisa diulang dari Settings → Kontrol.
- **Tombol menu dari mat:** setelah dipetakan, panah mat bisa menavigasi menu (atas/bawah memilih, kanan/Start = OK, kiri/Back = kembali) sehingga pengguna tidak perlu kembali ke keyboard.
- **Debounce per panel** (default ±15–20 ms, bisa diatur): mat lunak murah sering mengirim tekan-lepas ganda. Debounce tidak boleh menggeser waktu tekan pertama.
- **Waktu input presisi:** catat `Instant` saat event diterima dari gilrs (poll di thread input atau di awal frame) dan konversi ke `song_time` (§5.3). Ukur dan dokumentasikan latensi polling.
- **Linux:** gilrs membaca `/dev/input/event*`. Di Fedora, systemd memberi akses otomatis ke perangkat yang dikenali sebagai joystick (tag `uaccess`). Jika mat tidak terbaca, aplikasi menampilkan panduan ramah (bukan error mentah). Bila ternyata perlu aturan udev, paket RPM menyertakannya di `/usr/lib/udev/rules.d/`.
- **Windows:** tanpa driver tambahan untuk mat HID standar. Uji juga mat yang hanya terlihat di "Set up USB game controllers" (`joy.cpl`). Jika backend default gilrs tidak membacanya, laporkan ke pengguna dan usulkan solusi sebelum mengganti backend.
- **Panduan membeli** (untuk README/Settings): pilih mat berlabel **"USB / PC"** (kadang "PC/TV" dengan mode PC). Hindari mat *TV-only* yang punya game bawaan dan colokan AV saja. Mat wireless boleh jika dongle USB-nya terdeteksi sebagai gamepad.
- **Uji tanpa mat:** gamepad/stik apa pun bisa dipakai untuk menguji jalur ini. Di Fedora, cek perangkat dengan `evtest` (paket `evtest`) atau `jstest-gtk`.

---

## 6. Desain UI/UX — "modern, minimalis, ramah, memukau"

Referensi rasa (bukan untuk ditiru): kesederhanaan aplikasi musik modern, kehalusan animasi game ritme mobile modern, dan kebersihan antarmuka *settings* OS terbaru. Prinsip: **sedikit elemen, banyak ruang, gerak yang halus, umpan balik yang memuaskan.**

### 6.1 Design tokens (`ui/theme.rs`)

- **Tema gelap default** (opsi tema terang):
  - Latar: `#0B0B12` → gradien halus ke `#141428`
  - Permukaan kartu: `#1A1A2E` dengan transparansi ±85% dan border 1 px `#FFFFFF14`
  - Teks utama `#F2F2F7`, teks sekunder `#A0A0B8`
  - Aksen utama (gradien): `#7C5CFF` → `#3EC8FF`
  - Warna kolom (Left/Down/Up/Right): `#FF5C8A`, `#3EC8FF`, `#5CFFB0`, `#FFC85C`
  - Judgment: Marvelous `#E0F7FF` (berkilau), Perfect `#FFE45C`, Great `#5CFFB0`, Good `#3EC8FF`, Boo `#B07CFF`, Miss `#FF5C5C`
- **Tipografi:** Inter (bundel di `assets/fonts`, sertakan file lisensi OFL). Skala: 32/24/18/14/12. Angka skor memakai *tabular figures*.
- **Bentuk:** radius 16 px (kartu), 12 px (tombol), 999 px (pill). Spasi kelipatan 8.
- **Gerak:** durasi 150–300 ms, easing *ease-out cubic* / *spring* ringan. Semua di `ui/anim.rs`. Hormati opsi "Kurangi animasi".

### 6.2 Layar

1. **Title:** logo teks "Ferrostep" dengan gradien beranimasi pelan, partikel/gelombang halus yang berdenyut mengikuti BPM musik latar, tiga tombol besar: *Main*, *Impor Lagu*, *Pengaturan*.
2. **Song Select:** daftar lagu berbentuk kartu (bukan *wheel* jadul) dengan scroll yang halus berinersia; kolom kanan menampilkan judul besar, artis, BPM, durasi, dan *chip* difficulty berwarna dengan meter. Latar = warna dominan banner (jika ada) yang diburamkan, atau gradien yang diturunkan dari hash judul. Preview audio otomatis dengan fade. Pencarian instan (ketik untuk filter). Lagu hasil impor bertanda badge "Auto".
3. **Import:** zona *drop* besar dengan ikon dan teks "Seret MP3/MP4 ke sini atau klik untuk memilih". Saat proses: stepper 5 tahap dengan animasi centang, progress bar gradien, waveform lagu yang digambar setelah decode, lalu kartu hasil (BPM terdeteksi, jumlah note per difficulty) dengan tombol *Mainkan Sekarang*. Tombol batal selalu terlihat.
4. **Gameplay:** latar gelap tenang (opsional banner diburamkan, kecerahan bisa diatur). Receptor berbentuk panah vektor rounded dengan glow lembut yang "berdenyut" mengikuti beat. Saat hit: ledakan cahaya singkat berwarna judgment + partikel kecil; teks judgment muncul dengan *scale-pop*; combo besar di tengah dengan animasi angka. Progress lagu tipis di atas. Tidak ada HUD yang ramai.
5. **Results:** grade huruf besar beranimasi masuk, persentase dihitung naik, rincian judgment dalam bar horizontal, grafik sebaran timing (titik per note: sumbu-x waktu lagu, sumbu-y selisih ms), dan saran kalibrasi jika rata-rata selisih > ±10 ms ("Kamu cenderung menekan 18 ms terlambat. Sesuaikan offset?" + tombol satu klik).
6. **Settings:** kategori di sidebar (Gameplay, Audio, Tampilan, Kontrol, Folder). Bagian Kontrol menampilkan perangkat terhubung, status tiap panel (menyala saat diinjak), tombol "Atur ulang dance mat", dan pengaturan debounce. **Wizard kalibrasi offset**: putar klik metronom sintetis, pengguna menekan spasi mengikuti ketukan 16 kali, aplikasi menghitung rata-rata selisih dan mengusulkan global offset.

### 6.3 Kualitas & aksesibilitas

- Target **≥ 120 FPS** di gameplay pada laptop biasa; aktifkan vsync dengan opsi mati. Tampilkan FPS jika opsi debug aktif.
- Semua teks UI dalam **bahasa Indonesia** (siapkan struktur sederhana agar bisa diterjemahkan nanti).
- Navigasi penuh dengan keyboard (panah + Enter + Esc) selain mouse.
- Kontras teks memadai; ukuran UI bisa diskalakan (opsi 90–150%).
- Pesan error ramah dan memberi solusi, bukan *stack trace*.

### 6.4 Kemudahan untuk pengguna akhir (prioritas)

Pengguna akhir diasumsikan **bukan orang teknis**. Ia tidak perlu membuka terminal, mengedit file config, atau memasang apa pun selain aplikasinya sendiri.

- **Instalasi sekali klik:** Windows memakai installer `.msi`/`.exe` atau `.zip` portabel; Fedora memakai `.rpm` (klik dua kali → Discover/dnf) atau AppImage. Tidak ada dependensi yang harus dipasang manual (ffmpeg opsional dan tidak pernah diwajibkan).
- **Onboarding pertama kali (≤ 1 menit, bisa dilewati):** (1) pilih perangkat: keyboard atau dance mat (jika mat terdeteksi, langsung ke wizard pemetaan §5.9), (2) kalibrasi offset singkat, (3) tawarkan lagu contoh sintetis atau "Impor lagu pertamamu".
- **Default yang masuk akal:** No Fail aktif, CMod 500, volume 80%, judge normal. Pengguna bisa langsung main tanpa membuka Settings.
- **Daftar lagu kosong bukan jalan buntu:** tampilkan ajakan besar "Seret MP3 ke sini untuk mulai" + tombol Impor.
- **Petunjuk kontrol kontekstual** di bagian bawah layar, menyesuaikan perangkat aktif (ikon panah mat vs tombol keyboard).
- **Semua pengaturan tersimpan otomatis.** Tidak ada tombol "Simpan" yang bisa terlupa.
- **Masalah umum punya jawaban di dalam aplikasi:** "Suara terlambat?" → wizard kalibrasi; "Dance mat tidak terbaca?" → panduan langkah demi langkah per OS.

---

## 7. Tantangan teknis utama & solusinya

| Tantangan | Solusi |
|---|---|
| Sinkronisasi audio-visual & *drift* | AudioClock §5.3: jam audio sebagai sumber kebenaran, interpolasi monoton antar update buffer, global offset terkalibrasi, wizard kalibrasi. |
| UI freeze saat import | Semua decode/DSP/scan di thread terpisah; komunikasi via channel; UI hanya membaca pesan tiap frame (`try_recv`) dan memanggil `ctx.request_repaint()`. |
| Latensi audio | Buffer kecil di kira/cpal; opsi ukuran buffer di Settings. Jelaskan ke pengguna bahwa **headset Bluetooth menambah latensi besar** sehingga perlu kalibrasi. |
| Fedora: Wayland & PipeWire | eframe/winit mendukung Wayland & X11 secara native. Audio lewat ALSA → PipeWire (paket `pipewire-alsa` biasanya terpasang default di Fedora Workstation; verifikasi). Uji di sesi Wayland dan X11. |
| Windows: DX12/Vulkan | wgpu memilih backend otomatis; sediakan opsi env/config untuk memaksa backend jika ada masalah driver. |
| Kesalahan deteksi BPM (dobel/setengah) | Bobot preferensi tempo + tampilkan BPM hasil deteksi di layar import dengan tombol ×2 / ÷2 dan input manual sebelum menyimpan. |
| Kompatibilitas file dengan StepMania asli | Writer `.sm` mengikuti format standar; test round-trip; uji manual membuka hasil di StepMania 5 bila pengguna punya. |
| Ketersediaan ffmpeg | Opsional; symphonia sebagai jalur utama; fallback WAV. |
| Dance mat murah beragam | Pemetaan dari kode tombol/axis mentah + wizard + profil per VID/PID, debounce per panel, hot-plug (§5.9). Uji dengan gamepad apa pun sebelum mat dibeli. |

---

## 8. Strategi testing (pengguna TIDAK perlu alat khusus)

**Penting untuk disampaikan ke pengguna:** selama development tidak perlu *dance mat*. Cukup PC/laptop dengan keyboard dan speaker/headphone (lebih baik kabel daripada Bluetooth). Jalur dance mat bisa diuji dengan gamepad USB apa pun; setelah mat dibeli, uji dengan `evtest` lalu di dalam aplikasi.

### 8.1 Test otomatis (`cargo test`)

- **Parser:** fixture `.sm`/`.ssc` kecil buatan sendiri di `tests/fixtures/` (termasuk kasus aneh: komentar, CRLF, BOM, tag tak dikenal, measure 192 baris). Test round-trip writer.
- **Timing:** nilai yang dihitung manual untuk BPM tunggal, perubahan BPM, stop, offset.
- **DSP dengan audio sintetis (`audio_dsp/synth.rs`):** buat sinyal klik/metronom pada BPM diketahui (mis. 128, 140, 95,5) dengan offset diketahui dan sedikit noise, lalu assert BPM terdeteksi dalam ±0,1 dan offset dalam ±10 ms. Buat juga pola drum sintetis (kick di beat, hi-hat di off-beat) untuk menguji onset.
- **chart_gen:** property test sederhana: tidak ada pelanggaran aturan ergonomis, NPS dalam batas per difficulty, deterministik untuk seed yang sama.
- **Judgment:** input tekan pada selisih −200…+200 ms menghasilkan grade yang benar di tiap batas jendela.
- **Input:** pemetaan profil (tombol, hat, axis → kolom) dan debounce diuji dengan event palsu, tanpa perangkat fisik.

### 8.2 Test manual

- Generate file `Songs/_Test/Metronome/` (audio sintetis + chart di setiap beat) lewat perintah `cargo run -- --make-test-song`, untuk mengecek sinkronisasi dengan mata dan telinga.
- Impor beberapa lagu milik pengguna sendiri atau lagu bebas royalti dengan genre berbeda (EDM, pop, akustik) lalu nilai kenyamanan chart-nya.
- Jalankan di Windows 11 dan Fedora 45 (bila pengguna hanya punya satu OS: gunakan dual-boot, live USB Fedora, atau mesin virtual untuk uji tampilan; uji latensi audio terbaik di hardware asli). CI (§10) memastikan kode tetap terkompilasi di kedua OS.

### 8.3 Rutinitas sebelum commit

```
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

---

## 9. Roadmap bertahap (Definition of Done per fase)

Setiap fase diakhiri: semua test lulus, aplikasi bisa dijalankan, `docs/PROGRESS.md` diperbarui, commit Git dengan pesan jelas. Jelaskan ke pengguna cara mencoba hasil fase tersebut.

- **Fase 0 — Persiapan lingkungan (bersama pengguna)**
  - Pandu instalasi Rust lewat rustup. Windows: pasang juga *Visual Studio Build Tools* dengan workload "Desktop development with C++". Fedora: lihat §10.1.
  - `cargo init --name ferrostep` di folder repo yang sudah ada, inisialisasi Git, `.gitignore`.
  - Jendela eframe kosong dengan tema gelap dan teks "Halo, Ferrostep!" + bunyi beep lewat kira. Done: jendela muncul dan suara terdengar di OS pengguna.
- **Fase 1 — Parser & timing:** parse `.sm`/`.ssc`, writer `.sm`, modul timing, test lengkap. Belum ada UI baru.
- **Fase 2 — Gameplay inti:** AudioClock, notefield sederhana, modul `input` dengan keyboard (struktur sudah siap untuk gamepad), judgment, combo, layar hasil minimal. Pakai lagu test metronom. Done: bisa main satu lagu dari awal sampai akhir tanpa drift.
- **Fase 3 — Dance mat + Library & Song Select:** `gilrs` dengan hot-plug, wizard pemetaan, profil per perangkat, debounce, navigasi menu dari mat; pemindaian `Songs/`, daftar kartu, preview audio, pilih difficulty. Done: gamepad/mat dicolok saat aplikasi berjalan → terdeteksi → dipetakan → bisa memilih lagu dan main tanpa menyentuh keyboard.
- **Fase 4 — audio_dsp:** decode, onset, BPM, offset, generator sintetis + test akurasi.
- **Fase 5 — chart_gen + importer:** pipeline lengkap di background thread, layar Import dengan drag-and-drop, progress, batal, hot-reload. Done: seret MP3 → mainkan dalam < 30 detik untuk lagu 4 menit (target, ukur dan laporkan).
- **Fase 6 — Polish visual "memukau" & onboarding:** sistem desain penuh §6, animasi transisi, efek hit, results dengan grafik, wizard kalibrasi, onboarding pertama kali (§6.4), settings lengkap, keybind custom.
- **Fase 7 — Kualitas & kompatibilitas:** uji Wayland/X11 (KDE & GNOME)/Windows, opsi backend grafis, penanganan error ramah, performa ≥ 120 FPS, uji dance mat sungguhan di kedua OS (latensi, debounce, hot-plug).
- **Fase 8 — Packaging & rilis:** §10, README untuk pengguna akhir (termasuk panduan membeli & memasang dance mat), versi 0.1.0.
- **Fase lanjut (opsional):** BPM bervariasi, video latar dari MP4, editor chart sederhana, mode latihan (pelankan lagu), statistik riwayat.

---

## 10. Build, packaging, dan CI

### 10.1 Dependensi sistem Fedora 45

Nama paket sudah diverifikasi dengan `dnf repoquery` di Fedora 45 (2026-11-21). Pengguna yang menjalankan perintah ini sendiri:

```bash
sudo dnf install -y gcc gcc-c++ make pkgconf-pkg-config \
  alsa-lib-devel \
  wayland-devel libxkbcommon-devel \
  libX11-devel libXcursor-devel libXrandr-devel libXi-devel \
  vulkan-loader-devel mesa-vulkan-drivers \
  systemd-devel dbus-devel \
  evtest
```

`systemd-devel` menyediakan libudev yang dibutuhkan `gilrs`. `dbus-devel` dibutuhkan `kira` (fitur default `cpal-realtime-dbus`, yaitu prioritas realtime untuk thread audio lewat rtkit). `evtest` berguna untuk mengecek gamepad/dance mat. `openssl-devel` sengaja tidak dipasang; tambahkan hanya jika ternyata ada crate yang butuh. ffmpeg opsional: Fedora menyediakan `ffmpeg-free` di repo resmi, versi lengkap ada di RPM Fusion. Jelaskan perbedaannya ke pengguna bila relevan.

### 10.2 Windows 11

- Rust (toolchain `x86_64-pc-windows-msvc`) + Visual Studio Build Tools.
- Rilis: `cargo build --release` → `target/release/ferrostep.exe`. Distribusi sebagai `.zip` (exe + `assets/` + folder `Songs/` kosong + lisensi). Opsional installer `.msi` via `cargo-wix`.
- Set `#![windows_subsystem = "windows"]` untuk build release agar tidak muncul jendela konsol.
- Opsional: ikon aplikasi lewat `winres`/`embed-resource`.

### 10.3 Fedora 45

- `.rpm` via **`cargo-generate-rpm`** (konfigurasi di `[package.metadata.generate-rpm]` di Cargo.toml: binary, assets, file `.desktop`, ikon).
- Alternatif portabel: **AppImage** (mis. via `linuxdeploy` atau `cargo-appimage`; pilih yang terverifikasi bekerja).
- Sertakan file `ferrostep.desktop` dan ikon PNG/SVG agar muncul di menu aplikasi KDE Plasma maupun GNOME.
- Jika ternyata dibutuhkan, sertakan aturan udev untuk dance mat (§5.9) di paket RPM agar pengguna tidak perlu mengaturnya sendiri.

### 10.4 GitHub Actions (`.github/workflows/build.yml`)

- Matrix: `windows-latest` dan Linux. Untuk Linux, build di dalam **container `fedora:latest`** (atau versi Fedora yang sesuai) agar RPM dibuat di lingkungan Fedora. Catatan: Docker di CI tidak melanggar aturan "tanpa Docker", karena aturan itu berlaku untuk aplikasi yang didistribusikan.
- Langkah: checkout → pasang toolchain Rust → cache cargo → `cargo fmt --check` → `cargo clippy` → `cargo test` → `cargo build --release` → packaging → upload artifact. Trigger rilis saat push tag `v*`.

---

## 11. Aturan kerja untuk Claude Code

- Edisi Rust terbaru yang stabil; tanpa `unsafe` kecuali benar-benar perlu dan dijelaskan.
- Tidak ada `unwrap()`/`expect()` di jalur yang bisa gagal karena input pengguna atau file; gunakan `Result` dan tampilkan error ramah. `expect` boleh untuk invarian internal dengan pesan jelas.
- Fungsi kecil, nama deskriptif, komentar doc (`///`) pada item publik.
- Logika inti bebas dari UI dan bebas I/O bila memungkinkan, agar bisa diuji.
- Jangan menambah dependensi baru tanpa menyebutkan alasannya ke pengguna.
- Jangan menghapus atau menimpa isi `Songs/` milik pengguna. Importer hanya menulis ke `Songs/Imported/`.
- Commit kecil dan sering; pesan commit dalam bahasa Inggris, format `feat:`, `fix:`, `test:`, `docs:`, `refactor:`.
- Jika sebuah instruksi di dokumen ini ternyata salah atau tidak praktis setelah dicek, beri tahu pengguna dan usulkan revisi CLAUDE.md, jangan diam-diam menyimpang.

---

## 12. Glosarium singkat untuk pengguna

- **Crate:** paket/library Rust (seperti paket npm atau pip).
- **Cargo:** alat build & manajer paket Rust (`cargo run`, `cargo test`, `cargo build --release`).
- **Simfile:** file chart lagu (`.sm`/`.ssc`) berisi metadata, timing, dan susunan panah.
- **BPM:** ketukan per menit. **Offset:** posisi ketukan pertama relatif terhadap awal audio.
- **Onset:** titik awal sebuah bunyi (pukulan drum, petikan nada).
- **FFT/STFT:** cara mengubah sinyal suara menjadi komponen frekuensi per potongan waktu.
- **Judgment window:** batas toleransi (milidetik) untuk menilai ketepatan tekanan.
- **Jack:** note berturut-turut di kolom yang sama. **Jump:** dua panah bersamaan. **Crossover:** pola yang memaksa kaki menyilang.
- **Hot-reload:** daftar lagu diperbarui otomatis tanpa restart aplikasi.
