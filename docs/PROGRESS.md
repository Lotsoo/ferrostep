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

### Catatan teknis
- eframe 0.37: method wajib trait `App` adalah `fn ui(&mut self, ui: &mut egui::Ui, frame)` (bukan `update`).
- kira 0.12: `StaticSoundData` bisa dibuat langsung dari `Arc<[Frame]>` (field publik).
- Fitur default kira `cpal-realtime-dbus` butuh `dbus-devel` di Fedora.
- Warning `libvulkan_dzn.so` (driver Dozen) normal dan tidak berbahaya; log pustaka yang ramai difilter di `main.rs`.

### Berikutnya
- Jalankan `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, lalu commit pertama.
- Belum diuji di Windows 11.
- Fase 1: parser `.sm`/`.ssc`, writer `.sm`, modul `timing`, beserta test.

### Masalah terbuka
- Tidak ada.
