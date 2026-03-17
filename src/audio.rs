use anyhow::{Context, Result, bail, ensure};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, SampleRate, Stream, StreamConfig};
use hound::{SampleFormat as WavSampleFormat, WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

type SharedWriter = Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>;

pub struct RecordingSession {
    stream: Stream,
    writer: SharedWriter,
    working_dir: TempDir,
    wav_path: PathBuf,
}

pub struct RecordedClip {
    pub working_dir: TempDir,
    pub wav_path: PathBuf,
}

impl RecordingSession {
    pub fn start() -> Result<Self> {
        let working_dir = tempfile::Builder::new()
            .prefix("tvoice-")
            .tempdir()
            .context("failed to create temp directory for recording")?;

        let wav_path = working_dir.path().join("input.wav");
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no default microphone found")?;

        let input_config = device
            .default_input_config()
            .context("failed to read default microphone configuration")?;

        let stream_config = StreamConfig {
            channels: input_config.channels(),
            sample_rate: SampleRate(input_config.sample_rate().0),
            buffer_size: BufferSize::Default,
        };

        let writer = Arc::new(Mutex::new(Some(
            WavWriter::create(
                &wav_path,
                WavSpec {
                    channels: stream_config.channels,
                    sample_rate: stream_config.sample_rate.0,
                    bits_per_sample: 16,
                    sample_format: WavSampleFormat::Int,
                },
            )
            .with_context(|| format!("failed to create wav file at {}", wav_path.display()))?,
        )));

        let writer_for_stream = Arc::clone(&writer);
        let error_callback = |error| eprintln!("audio stream error: {error}");

        let stream = match input_config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| write_f32_samples(data, &writer_for_stream),
                error_callback,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| write_i16_samples(data, &writer_for_stream),
                error_callback,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| write_u16_samples(data, &writer_for_stream),
                error_callback,
                None,
            ),
            other => bail!("unsupported microphone sample format: {other:?}"),
        }
        .context("failed to open microphone stream")?;

        stream.play().context("failed to start microphone stream")?;

        Ok(Self {
            stream,
            writer,
            working_dir,
            wav_path,
        })
    }

    pub fn stop(self) -> Result<RecordedClip> {
        drop(self.stream);

        let mut guard = self.writer.lock().expect("recorder writer mutex poisoned");
        let writer = guard.take().context("wav writer was already finalized")?;
        writer.finalize().context("failed to finalize wav file")?;
        drop(guard);

        Ok(RecordedClip {
            working_dir: self.working_dir,
            wav_path: self.wav_path,
        })
    }
}

pub fn compress_to_m4a(clip: &RecordedClip) -> Result<PathBuf> {
    let m4a_path = clip.working_dir.path().join("input.m4a");

    let output = Command::new("afconvert")
        .arg("-f")
        .arg("m4af")
        .arg("-d")
        .arg("aac")
        .arg("-b")
        .arg("64000")
        .arg(&clip.wav_path)
        .arg(&m4a_path)
        .output()
        .context("failed to spawn afconvert")?;

    ensure!(
        output.status.success(),
        "afconvert failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );

    Ok(m4a_path)
}

fn write_f32_samples(data: &[f32], writer: &SharedWriter) {
    if let Ok(mut guard) = writer.lock() {
        if let Some(writer) = guard.as_mut() {
            for sample in data {
                let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
                if writer.write_sample(value).is_err() {
                    eprintln!("failed to write microphone sample");
                    break;
                }
            }
        }
    }
}

fn write_i16_samples(data: &[i16], writer: &SharedWriter) {
    if let Ok(mut guard) = writer.lock() {
        if let Some(writer) = guard.as_mut() {
            for sample in data {
                if writer.write_sample(*sample).is_err() {
                    eprintln!("failed to write microphone sample");
                    break;
                }
            }
        }
    }
}

fn write_u16_samples(data: &[u16], writer: &SharedWriter) {
    if let Ok(mut guard) = writer.lock() {
        if let Some(writer) = guard.as_mut() {
            for sample in data {
                let value = (*sample as i32 - 32_768) as i16;
                if writer.write_sample(value).is_err() {
                    eprintln!("failed to write microphone sample");
                    break;
                }
            }
        }
    }
}
