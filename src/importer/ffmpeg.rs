//! Finding and running the optional `ffmpeg` command-line tool.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::audio_dsp::decode::Cancelled;

#[cfg(windows)]
const EXE: &str = "ffmpeg.exe";
#[cfg(not(windows))]
const EXE: &str = "ffmpeg";

/// Looks for ffmpeg: (1) path from settings, (2) next to our executable, (3) `PATH`.
pub fn find_ffmpeg(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = configured
        && p.is_file()
    {
        return Some(p.to_path_buf());
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        let p = dir.join(EXE);
        if p.is_file() {
            return Some(p);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(EXE))
        .find(|p| p.is_file())
}

/// Converts the audio track of `input` to Ogg Vorbis. Can be cancelled.
pub fn convert_to_ogg(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-y", "-v", "error", "-i"])
        .arg(input)
        .args(["-vn", "-map", "0:a:0", "-c:a", "libvorbis", "-q:a", "5"])
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // Do not flash a console window.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().context("ffmpeg tidak bisa dijalankan")?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Cancelled.into());
        }
        if let Some(status) = child.try_wait()? {
            if status.success() && output.is_file() {
                return Ok(());
            }
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                use std::io::Read;
                let _ = stderr.read_to_string(&mut err);
            }
            bail!(
                "ffmpeg gagal: {}",
                err.lines().last().unwrap_or("tanpa pesan")
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
