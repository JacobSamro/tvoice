#[cfg(target_os = "macos")]
mod audio;
#[cfg(target_os = "macos")]
mod openrouter;
#[cfg(target_os = "macos")]
mod paste;

#[cfg(target_os = "macos")]
use anyhow::{Context, Result, anyhow};
#[cfg(target_os = "macos")]
use audio::{RecordedClip, RecordingSession, compress_to_m4a};
#[cfg(target_os = "macos")]
use core_foundation::runloop::CFRunLoop;
#[cfg(target_os = "macos")]
use core_graphics::event::{
    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult, EventField, KeyCode,
};
#[cfg(target_os = "macos")]
use openrouter::{OpenRouterClient, OpenRouterConfig};
#[cfg(target_os = "macos")]
use std::env;
#[cfg(target_os = "macos")]
use std::sync::mpsc;
#[cfg(target_os = "macos")]
use std::thread;

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    run()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("tvoice currently supports macOS only.");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn run() -> Result<()> {
    let _ = dotenvy::dotenv();
    let config = AppConfig::from_env()?;
    let (control_tx, control_rx) = mpsc::channel();
    let (clip_tx, clip_rx) = mpsc::channel();

    spawn_recording_controller(control_rx, clip_tx);
    spawn_processing_worker(config.openrouter.clone(), clip_rx);

    println!("tvoice is running.");
    println!("Hold Control to record, release to transcribe and paste.");
    println!(
        "Grant Microphone, Accessibility, and Input Monitoring permissions when macOS prompts."
    );

    install_control_hold_event_tap(control_tx)
}

#[cfg(target_os = "macos")]
fn spawn_recording_controller(
    control_rx: mpsc::Receiver<ControlMessage>,
    clip_tx: mpsc::Sender<RecordedClip>,
) {
    thread::spawn(move || {
        let mut active_recording: Option<RecordingSession> = None;

        while let Ok(message) = control_rx.recv() {
            match message {
                ControlMessage::StartRecording => {
                    if active_recording.is_none() {
                        match RecordingSession::start() {
                            Ok(recording) => active_recording = Some(recording),
                            Err(error) => eprintln!("failed to start recording: {error:#}"),
                        }
                    }
                }
                ControlMessage::StopRecording => {
                    if let Some(recording) = active_recording.take() {
                        match recording.stop() {
                            Ok(clip) => {
                                if clip_tx.send(clip).is_err() {
                                    break;
                                }
                            }
                            Err(error) => eprintln!("failed to stop recording: {error:#}"),
                        }
                    }
                }
            }
        }
    });
}

#[cfg(target_os = "macos")]
fn spawn_processing_worker(
    openrouter_config: OpenRouterConfig,
    clip_rx: mpsc::Receiver<RecordedClip>,
) {
    thread::spawn(move || {
        let client = match OpenRouterClient::new(openrouter_config) {
            Ok(client) => client,
            Err(error) => {
                eprintln!("failed to initialize OpenRouter client: {error:#}");
                return;
            }
        };

        while let Ok(clip) = clip_rx.recv() {
            if let Err(error) = process_clip(&client, clip) {
                eprintln!("failed to process clip: {error:#}");
            }
        }
    });
}

#[cfg(target_os = "macos")]
fn process_clip(client: &OpenRouterClient, clip: RecordedClip) -> Result<()> {
    let compressed_audio = compress_to_m4a(&clip)?;
    let transcript = client.transcribe_file(&compressed_audio, "m4a")?;

    if transcript.is_empty() {
        return Ok(());
    }

    paste::paste_text(&transcript)?;
    println!("transcribed {} characters", transcript.chars().count());

    Ok(())
}

#[cfg(target_os = "macos")]
fn install_control_hold_event_tap(control_tx: mpsc::Sender<ControlMessage>) -> Result<()> {
    let event_mask = vec![CGEventType::FlagsChanged];

    let blocked_modifiers = CGEventFlags::CGEventFlagCommand
        | CGEventFlags::CGEventFlagAlternate
        | CGEventFlags::CGEventFlagShift
        | CGEventFlags::CGEventFlagSecondaryFn;

    CGEventTap::with_enabled(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        event_mask,
        move |_proxy, event_type, event| {
            let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            if keycode != KeyCode::CONTROL && keycode != KeyCode::RIGHT_CONTROL {
                return CallbackResult::Keep;
            }

            if event.get_flags().intersects(blocked_modifiers) {
                return CallbackResult::Keep;
            }

            let message = match event_type {
                CGEventType::FlagsChanged => {
                    if event.get_flags().contains(CGEventFlags::CGEventFlagControl) {
                        ControlMessage::StartRecording
                    } else {
                        ControlMessage::StopRecording
                    }
                }
                _ => return CallbackResult::Keep,
            };

            if control_tx.send(message).is_ok() {
                CallbackResult::Drop
            } else {
                CallbackResult::Keep
            }
        },
        || CFRunLoop::run_current(),
    )
    .map_err(|_| anyhow!("failed to install the global Control-key event tap"))?;

    Ok(())
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct AppConfig {
    openrouter: OpenRouterConfig,
}

#[cfg(target_os = "macos")]
impl AppConfig {
    fn from_env() -> Result<Self> {
        let api_key = env::var("OPENROUTER_API_KEY")
            .context("OPENROUTER_API_KEY is required to call OpenRouter")?;

        let model =
            env::var("TVOICE_OPENROUTER_MODEL").unwrap_or_else(|_| "openai/gpt-audio-mini".into());
        let app_title = env::var("TVOICE_APP_TITLE").unwrap_or_else(|_| "tvoice".into());
        let http_referer = env::var("TVOICE_HTTP_REFERER").ok();

        Ok(Self {
            openrouter: OpenRouterConfig {
                api_key,
                model,
                app_title,
                http_referer,
            },
        })
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
enum ControlMessage {
    StartRecording,
    StopRecording,
}
