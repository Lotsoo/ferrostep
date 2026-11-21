# Tanda tangan digital rilis Windows (SignPath Foundation)

Tujuan: `ferrostep.exe` dan installer tidak lagi memunculkan peringatan
"Windows protected your PC" (SmartScreen) karena belum ditandatangani.

SignPath Foundation memberi sertifikat **gratis** untuk proyek open-source,
dengan syarat (ringkasan dari <https://signpath.org/terms>):

- lisensi OSI tanpa lisensi ganda komersial (Ferrostep: MIT ✔),
- tidak ada kode proprietary (semua dependensi open-source ✔),
- repo publik, dipelihara aktif, dan **sudah pernah dirilis** dalam bentuk yang akan ditandatangani,
- fungsi aplikasi dijelaskan di halaman unduhan (README ✔),
- bagian **"Code signing policy"** + pernyataan privasi di halaman proyek (README ✔, tinggal isi username),
- semua anggota tim memakai **autentikasi dua faktor (2FA)** di GitHub dan SignPath,
- binary dibangun dari source oleh CI di runner milik GitHub (workflow ✔),
- setiap rilis disetujui manual sebelum ditandatangani,
- installer punya cara uninstall (✔) dan memberitahu perubahan sistem (✔ di halaman sambutan installer).

Yang ditandai ✔ sudah disiapkan di repo. Sisanya dikerjakan pemilik proyek.

## Langkah-langkah

1. **Aktifkan 2FA di GitHub**: Settings → Password and authentication → Two-factor authentication.
2. **Buat repo publik** di GitHub bernama `ferrostep` (kosong, tanpa README/lisensi bawaan).
3. Ganti `GITHUB_USERNAME` di bagian *Code signing policy* pada `README.md` dengan username GitHub-mu.
4. **Push** repo ini ke GitHub (Claude Code bisa membantu setelah repo dibuat).
5. **Rilis pertama tanpa tanda tangan**: buat tag `v0.1.0` dan push. CI membuat GitHub Release
   berisi zip + installer + RPM. Ini memenuhi syarat "sudah pernah dirilis".
6. **Daftar** di <https://signpath.org/apply>. Isi dengan link repo dan halaman Release.
   Persetujuan ditentukan SignPath; tidak ada jaminan waktu.
7. Setelah disetujui, di dasbor SignPath:
   - buat project dengan slug **`ferrostep`** dan hubungkan Trusted Build System **GitHub.com**;
   - buat signing policy dengan slug **`release-signing`** (approval manual);
   - buat dua artifact configuration: **`exe`** (untuk `ferrostep.exe`) dan **`installer`**
     (untuk `Ferrostep-Setup-*.exe`). Keduanya berada di dalam file zip artifact GitHub,
     dan wajib membatasi metadata *product name* = `Ferrostep`;
   - buat API token untuk user dengan peran *submitter*;
   - (disarankan) pasang SignPath GitHub App: <https://github.com/apps/signpath>.
8. Di GitHub → repo → Settings → Secrets and variables → Actions:
   - **Secret** `SIGNPATH_API_TOKEN` = API token dari langkah 7,
   - **Variable** `SIGNPATH_ORGANIZATION_ID` = Organization ID SignPath.
9. Rilis berikutnya (mis. tag `v0.1.1`): CI berhenti menunggu di langkah *Sign exe* dan
   *Sign installer* sampai kamu menyetujui permintaan di SignPath (batas 1 jam per langkah).

## Catatan

- Nama penerbit yang tampil di Windows adalah **"SignPath Foundation"**, bukan nama pribadi.
- Build biasa (push ke `main`, pull request) tidak pernah ditandatangani dan tidak butuh persetujuan.
- Jika slug di SignPath dibuat berbeda, sesuaikan `project-slug`, `signing-policy-slug`, dan
  `artifact-configuration-slug` di `.github/workflows/build.yml`.
