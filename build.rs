//! Build script:
//! - copies the optional private welcome letter (`private/surat.txt`, never
//!   committed) into the build, so only builds made on the owner's machine
//!   contain it;
//! - on Windows targets, embeds the app icon and version info into
//!   `ferrostep.exe` so Explorer, the taskbar, shortcuts and "Apps" show the
//!   Ferrostep icon and name.

use std::path::Path;

fn main() {
    embed_letter();
    println!("cargo:rerun-if-changed=assets/icons/ferrostep.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icons/ferrostep.ico")
        .set("ProductName", "Ferrostep")
        .set("FileDescription", "Ferrostep")
        .set("CompanyName", "Ferrostep")
        .set("LegalCopyright", "MIT License");
    // Cross-compiling from Linux with the MinGW toolchain.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") && !cfg!(windows) {
        res.set_toolkit_path("/usr/bin")
            .set_windres_path("x86_64-w64-mingw32-windres")
            .set_ar_path("x86_64-w64-mingw32-ar");
    }
    if let Err(e) = res.compile() {
        // A missing icon must not break the build; the exe just has no icon.
        println!("cargo:warning=could not embed the Windows icon: {e}");
    }
}

/// Writes the letter (or an empty file) to `$OUT_DIR/letter.txt`. Only
/// rewrites when the content changed, so normal builds stay incremental.
fn embed_letter() {
    let source = Path::new("private/surat.txt");
    println!("cargo:rerun-if-changed=private");
    println!("cargo:rerun-if-changed=private/surat.txt");
    let text = std::fs::read_to_string(source).unwrap_or_default();
    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return;
    };
    let target = Path::new(&out_dir).join("letter.txt");
    if std::fs::read_to_string(&target).ok().as_deref() != Some(text.as_str()) {
        let _ = std::fs::write(&target, text);
    }
}
