//! Decodes audio files (MP3, MP4/AAC, OGG, WAV, FLAC) to mono `f32` with symphonia.

use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardTag};

/// Decoded audio plus tags.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    /// Mono mixdown in −1..=1.
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: usize,
    pub title: Option<String>,
    pub artist: Option<String>,
}

impl DecodedAudio {
    pub fn duration(&self) -> f64 {
        self.samples.len() as f64 / f64::from(self.sample_rate.max(1))
    }
}

/// Error used when the user cancels.
#[derive(Debug, thiserror::Error)]
#[error("dibatalkan")]
pub struct Cancelled;

/// Decodes `path`. If `wav_out` is given, also writes a 16-bit stereo WAV copy while decoding.
pub fn decode_file(
    path: &Path,
    wav_out: Option<&Path>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<DecodedAudio> {
    let file =
        File::open(path).with_context(|| format!("tidak bisa membuka {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("format audio tidak dikenali")?;

    let (title, artist) = read_tags(&mut format.metadata());

    let track = format
        .default_track(TrackType::Audio)
        .context("file tidak berisi trek audio")?;
    let track_id = track.id;
    let total_frames = track.num_frames.unwrap_or(0);
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .context("parameter codec audio tidak ada")?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .context("codec audio tidak didukung")?;

    let mut samples: Vec<f32> = Vec::new();
    let mut interleaved: Vec<f32> = Vec::new();
    let mut sample_rate = 0u32;
    let mut channels = 0usize;
    let mut wav: Option<hound::WavWriter<std::io::BufWriter<File>>> = None;
    let mut frames_done: u64 = 0;
    let mut tags = (title, artist);

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Cancelled.into());
        }
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => break,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(e) => return Err(e).context("gagal membaca paket audio"),
        };
        if tags.0.is_none() || tags.1.is_none() {
            let (t, a) = read_tags(&mut format.metadata());
            tags.0 = tags.0.or(t);
            tags.1 = tags.1.or(a);
        }
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            Err(SymphoniaError::DecodeError(e)) => {
                log::debug!("skipping bad packet: {e}");
                continue;
            }
            Err(SymphoniaError::IoError(_)) => continue,
            Err(e) => return Err(e).context("gagal mendekode audio"),
        };
        if sample_rate == 0 {
            sample_rate = buf.spec().rate();
            channels = buf.spec().channels().count().max(1);
            if let Some(out) = wav_out {
                let spec = hound::WavSpec {
                    channels: channels.min(2) as u16,
                    sample_rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                wav = Some(
                    hound::WavWriter::create(out, spec)
                        .with_context(|| format!("tidak bisa menulis {}", out.display()))?,
                );
            }
        }
        buf.copy_to_vec_interleaved(&mut interleaved);
        let frames = interleaved.len() / channels;
        for frame in interleaved.chunks_exact(channels) {
            samples.push(frame.iter().sum::<f32>() / channels as f32);
            if let Some(w) = wav.as_mut() {
                for &s in frame.iter().take(2) {
                    w.write_sample((s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)?;
                }
            }
        }
        frames_done += frames as u64;
        if total_frames > 0 {
            progress((frames_done as f32 / total_frames as f32).min(1.0));
        }
    }
    if let Some(w) = wav {
        w.finalize()?;
    }
    if samples.is_empty() || sample_rate == 0 {
        bail!("file audio kosong atau tidak bisa didekode");
    }
    progress(1.0);
    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
        title: tags.0,
        artist: tags.1,
    })
}

fn read_tags(meta: &mut symphonia::core::meta::Metadata<'_>) -> (Option<String>, Option<String>) {
    let (mut title, mut artist) = (None, None);
    if let Some(rev) = meta.skip_to_latest() {
        for tag in &rev.media.tags {
            match &tag.std {
                Some(StandardTag::TrackTitle(s)) if !s.trim().is_empty() => {
                    title = Some(s.trim().to_string())
                }
                Some(StandardTag::Artist(s)) if !s.trim().is_empty() => {
                    artist = Some(s.trim().to_string())
                }
                _ => {}
            }
        }
    }
    (title, artist)
}
