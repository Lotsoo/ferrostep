# Ferrostep

Game ritme 4 panah yang modern untuk **Windows 11** dan **Fedora Linux**.
Main dengan keyboard atau **dance mat USB** yang langsung terdeteksi saat dicolok.
Ubah lagu MP3/MP4 milikmu menjadi chart otomatis dalam hitungan detik.

- Memainkan simfile StepMania (`.sm` / `.ssc`).
- **Impor MP3/MP4 → chart otomatis**: BPM, offset, dan 4 tingkat kesulitan (Pemula, Mudah, Sedang, Sulit).
- **Dance mat plug and play**: colok, injak 4 panah sekali untuk pemetaan, lalu langsung main. Bisa dipakai juga untuk menavigasi menu.
- Kalibrasi offset sekali ketuk, layar hasil dengan grafik timing, dan tema gelap/terang.

---

## Memasang

### Windows 11

1. Unduh `Ferrostep-windows-x86_64.zip` dari halaman **Releases**.
2. Ekstrak ke folder mana saja (mis. `Documents\Ferrostep`).
3. Klik dua kali `ferrostep.exe`.
   Jika muncul *Windows protected your PC*, klik **More info → Run anyway** (wajar untuk aplikasi baru yang belum ditandatangani).

Lagu disimpan di folder `Songs` di samping `ferrostep.exe`.

### Fedora

1. Unduh file `ferrostep-*.x86_64.rpm` dari halaman **Releases**.
2. Klik dua kali file tersebut (terbuka di Discover/Software), lalu **Install**.
   Atau lewat terminal: `sudo dnf install ./ferrostep-*.x86_64.rpm`
3. Buka **Ferrostep** dari menu aplikasi.

Lagu disimpan di `~/.local/share/ferrostep/Songs`. Bisa diganti di **Pengaturan → Folder**.

---

## Bermain

| Panah | Keyboard (default) |
|---|---|
| Kiri | `D` atau `←` |
| Bawah | `F` atau `↓` |
| Atas | `J` atau `↑` |
| Kanan | `K` atau `→` |

Menu: panah untuk memilih, `Enter` untuk masuk, `Esc` untuk kembali. Semua tombol bisa diubah di **Pengaturan → Kontrol**.

**Menambah lagu:** buka **Impor Lagu**, lalu pilih atau seret file MP3/MP4. Lagu siap dimainkan dalam beberapa detik.
Jika panah terasa terlalu rapat atau jarang, tekan **÷2** atau **×2** di kartu hasil impor.

**Suara terasa terlambat?** Buka **Pengaturan → Audio → Kalibrasi offset**, lalu ketuk mengikuti klik 16 kali.
Headset Bluetooth hampir selalu butuh kalibrasi; speaker/headphone kabel lebih akurat.

---

## Dance mat

### Memilih saat membeli

- Pilih mat berlabel **"USB"**, **"PC"**, atau **"PC/TV"** (yang punya mode PC).
- **Hindari** mat *khusus TV* yang hanya punya colokan AV/RCA dan game bawaan: mat seperti ini tidak bisa dibaca komputer.
- Mat nirkabel boleh, asalkan dongle USB-nya terdeteksi sebagai *gamepad*.

### Memakai

1. Colokkan mat ke port USB, kapan saja, bahkan saat Ferrostep sudah terbuka.
2. Muncul notifikasi **"Dance mat terhubung"**. Untuk mat yang baru pertama kali dipakai, Ferrostep meminta kamu menginjak panah **kiri, bawah, atas, kanan**.
3. Selesai. Mat dikenali otomatis di kemudian hari.

Jika satu injakan terbaca dua kali, naikkan **Debounce panel** di **Pengaturan → Kontrol**.

### Mat tidak terbaca?

- **Windows:** ketik `joy.cpl` di Start. Jika mat muncul di jendela *Game Controllers*, Ferrostep juga bisa membacanya. Coba port USB lain dan hindari hub tanpa daya.
- **Fedora:** jalankan `sudo dnf install evtest`, lalu `evtest`. Mat harus muncul di daftar. Jika mat hanya terbaca dengan `sudo`, laporkan nama perangkatnya agar bisa dibuatkan aturan udev.
- Beberapa mat punya tombol mode (analog/digital atau PC/TV). Coba tekan **Select + Start** bersamaan.

Panduan yang sama ada di aplikasi: **Pengaturan → Kontrol → Dance mat tidak terbaca?**

---

## Masalah umum

| Masalah | Solusi |
|---|---|
| Layar hitam atau aplikasi tidak terbuka | Ferrostep menawarkan mode grafis cadangan (OpenGL). Pilih **Ya**, lalu buka lagi. Bisa juga diganti di **Pengaturan → Tampilan → Backend grafis**. |
| Tidak ada suara | Cek perangkat output di pengaturan suara OS, lalu buka ulang Ferrostep. |
| File MP4 hasil impor besar | Pasang ffmpeg (opsional). Fedora: `sudo dnf install ffmpeg-free`. Windows: taruh `ffmpeg.exe` di samping `ferrostep.exe`. |
| Aplikasi tertutup sendiri | Kirim file `crash.log` dari folder konfigurasi (Fedora: `~/.config/ferrostep/`, Windows: `%APPDATA%\ferrostep\config\`). |
| Seret file tidak berfungsi di Fedora (Wayland) | Sistem jendela yang dipakai belum mendukung *drag-and-drop* di Wayland. Klik zona impor untuk memilih file. |

---

## Untuk pengembang

Ditulis dalam Rust (egui/eframe + wgpu, kira, symphonia, gilrs). Arsitektur dijelaskan di [`CLAUDE.md`](CLAUDE.md) dan log kemajuan ada di [`docs/PROGRESS.md`](docs/PROGRESS.md).

### Fedora

```bash
sudo dnf install -y gcc gcc-c++ make pkgconf-pkg-config alsa-lib-devel \
  wayland-devel libxkbcommon-devel libX11-devel libXcursor-devel libXrandr-devel libXi-devel \
  vulkan-loader-devel mesa-vulkan-drivers systemd-devel dbus-devel evtest
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

cargo run                       # jalankan
cargo test                      # semua test (tanpa perangkat/audio)
cargo run -- --make-test-song   # buat lagu metronom di Songs/_Test/
RUST_LOG=ferrostep=debug cargo run   # log rinci, termasuk FPS
cargo run -- --play "Songs/_Test/Metronome" --autoplay   # langsung main, game menekan sendiri
cargo run -- --help             # semua opsi
```

Membuat RPM:

```bash
cargo install cargo-generate-rpm
cargo build --release
cargo generate-rpm              # hasil: target/generate-rpm/*.rpm
```

### Windows

1. Pasang [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) dengan workload **"Desktop development with C++"**.
2. Pasang Rust dari [rustup.rs](https://rustup.rs) (toolchain `x86_64-pc-windows-msvc`).
3. `cargo run` atau `cargo build --release` (hasil: `target\release\ferrostep.exe`).

### CI

`.github/workflows/build.yml` menjalankan fmt, clippy, dan test, membuat RPM (di container Fedora) dan zip Windows.
Push tag `v*` (mis. `v0.1.0`) membuat GitHub Release otomatis.

---

## Lisensi

Kode: MIT (lihat [`LICENSE`](LICENSE)). Font Inter: SIL Open Font License (`assets/fonts/LICENSE-Inter.txt`).
Ferrostep tidak menyertakan lagu berhak cipta, dan tidak berafiliasi dengan StepMania, Etterna, atau DDR.
