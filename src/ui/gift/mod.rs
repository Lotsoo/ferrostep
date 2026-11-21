//! Private birthday gift (private builds only): the welcome letter ([`letter`])
//! and the greeting videos shown after it.
//!
//! `build.rs` converts `private/video/*.mp4` into JPEG frame packs plus Ogg
//! audio and embeds them; public builds embed nothing and never show this.
//! The videos play one after another in a card with no controls at all: no
//! skip, no pause, no seeking, and the window cannot be closed meanwhile.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use crossbeam_channel::Receiver;

use eframe::egui::{
    self, Align2, Color32, ColorImage, Rect, Sense, TextureHandle, TextureOptions, Ui, pos2, vec2,
};
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use super::theme::{self, ACCENT_A, ACCENT_B, Palette, column_color, font, font_bold};
use super::{anim, notefield, widgets};
use crate::audio::{Audio, AudioClock, Music};

mod decoder;
mod greetings;
pub mod letter;

pub use greetings::Greetings;

/// One embedded greeting video.
pub struct GiftVideo {
    /// Shown as "Ucapan dari <name>".
    pub name: &'static str,
    /// Frame pack written by `build.rs` (`FSV1` format).
    pub frames: &'static [u8],
    /// Ogg Vorbis soundtrack.
    pub audio: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/gift_media.rs"));
