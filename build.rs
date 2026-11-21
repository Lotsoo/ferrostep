//! Build script: on Windows targets, embeds the app icon and version info into
//! `ferrostep.exe` so Explorer, the taskbar, shortcuts and "Apps" show the
//! Ferrostep icon and name. Does nothing for other targets.

fn main() {
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
