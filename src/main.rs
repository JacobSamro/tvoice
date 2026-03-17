#[cfg(target_os = "macos")]
mod audio;
#[cfg(target_os = "macos")]
mod openrouter;
#[cfg(target_os = "macos")]
mod paste;
#[cfg(target_os = "macos")]
mod settings;
#[cfg(target_os = "macos")]
mod status_item;
#[cfg(target_os = "macos")]
mod ui;

#[cfg(target_os = "macos")]
use anyhow::{Result, anyhow, ensure};
#[cfg(target_os = "macos")]
use audio::{RecordedClip, RecordingSession, compress_to_m4a};
#[cfg(target_os = "macos")]
use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicy};
#[cfg(target_os = "macos")]
use cocoa::base::nil;
#[cfg(target_os = "macos")]
use core_foundation::runloop::CFRunLoop;
#[cfg(target_os = "macos")]
use core_graphics::event::{
    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult, EventField, KeyCode,
};
#[cfg(target_os = "macos")]
use gpui::{App, Application, Global};
#[cfg(target_os = "macos")]
use openrouter::{OpenRouterClient, OpenRouterConfig};
#[cfg(target_os = "macos")]
use settings::{AppSettings, SharedSettings};
#[cfg(target_os = "macos")]
use std::env;
#[cfg(target_os = "macos")]
use std::sync::{Arc, RwLock, mpsc};
#[cfg(target_os = "macos")]
use std::thread;
#[cfg(target_os = "macos")]
use ui::{UiCommand, display_uuid_for_point};
#[cfg(target_os = "macos")]
use uuid::Uuid;

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn TransformProcessType(psn: *const ProcessSerialNumber, transform_state: i32) -> i32;
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct ProcessSerialNumber {
    high_long_of_psn: u32,
    low_long_of_psn: u32,
}

#[cfg(target_os = "macos")]
const K_CURRENT_PROCESS: u32 = 2;
#[cfg(target_os = "macos")]
const K_PROCESS_TRANSFORM_TO_FOREGROUND_APPLICATION: i32 = 1;

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
    if !is_running_from_app_bundle()
        && let Err(error) = promote_terminal_process_to_foreground_app()
    {
        eprintln!("failed to promote the current process into a foreground macOS app: {error:#}");
    }
    let settings = Arc::new(RwLock::new(AppSettings::load()?));

    Application::new().run(move |cx: &mut App| {
        configure_menu_bar_app();
        gpui_component::init(cx);

        let (ui_tx, ui_rx) = mpsc::channel();
        ui::spawn_ui_command_loop(cx, Arc::clone(&settings), ui_rx);

        match status_item::StatusItemController::install(ui_tx.clone()) {
            Ok(status_item) => cx.set_global(AppServices {
                _status_item: status_item,
            }),
            Err(error) => {
                eprintln!("failed to install the menu bar item: {error:#}");
                let _ = ui_tx.send(UiCommand::ShowSettings);
            }
        }

        if let Err(error) = ui::open_settings_window_now(cx, Arc::clone(&settings)) {
            eprintln!("failed to open the initial settings window: {error:#}");
            let _ = ui_tx.send(UiCommand::ShowSettings);
        }

        let (control_tx, control_rx) = mpsc::channel();
        let (clip_tx, clip_rx) = mpsc::channel();

        spawn_recording_controller(control_rx, clip_tx, ui_tx.clone());
        spawn_processing_worker(Arc::clone(&settings), clip_rx);
        spawn_control_event_tap(control_tx);

        eprintln!("tvoice is running from the menu bar.");
        eprintln!("Hold Control to record, release to transcribe and paste.");
        eprintln!(
            "Grant Microphone, Accessibility, and Input Monitoring permissions when macOS prompts."
        );
    });

    Ok(())
}

#[cfg(target_os = "macos")]
fn is_running_from_app_bundle() -> bool {
    env::current_exe()
        .ok()
        .and_then(|path| path.to_str().map(str::to_owned))
        .map(|path| path.contains(".app/Contents/MacOS/"))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn promote_terminal_process_to_foreground_app() -> Result<()> {
    let psn = ProcessSerialNumber {
        high_long_of_psn: 0,
        low_long_of_psn: K_CURRENT_PROCESS,
    };

    let status = unsafe { TransformProcessType(&psn, K_PROCESS_TRANSFORM_TO_FOREGROUND_APPLICATION) };
    ensure!(
        status == 0,
        "TransformProcessType returned OSStatus {status}"
    );

    Ok(())
}

#[cfg(target_os = "macos")]
fn spawn_recording_controller(
    control_rx: mpsc::Receiver<ControlMessage>,
    clip_tx: mpsc::Sender<RecordedClip>,
    ui_tx: mpsc::Sender<UiCommand>,
) {
    thread::spawn(move || {
        let mut active_recording: Option<RecordingSession> = None;

        while let Ok(message) = control_rx.recv() {
            match message {
                ControlMessage::StartRecording { display_uuid } => {
                    if active_recording.is_none() {
                        match RecordingSession::start() {
                            Ok(recording) => {
                                active_recording = Some(recording);
                                let _ = ui_tx.send(UiCommand::ShowOverlay { display_uuid });
                            }
                            Err(error) => eprintln!("failed to start recording: {error:#}"),
                        }
                    }
                }
                ControlMessage::StopRecording => {
                    if let Some(recording) = active_recording.take() {
                        let _ = ui_tx.send(UiCommand::HideOverlay);
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
    settings: SharedSettings,
    clip_rx: mpsc::Receiver<RecordedClip>,
) {
    thread::spawn(move || {
        let client = match OpenRouterClient::new() {
            Ok(client) => client,
            Err(error) => {
                eprintln!("failed to initialize OpenRouter client: {error:#}");
                return;
            }
        };

        while let Ok(clip) = clip_rx.recv() {
            if let Err(error) = process_clip(&client, &settings, clip) {
                eprintln!("failed to process clip: {error:#}");
            }
        }
    });
}

#[cfg(target_os = "macos")]
fn process_clip(
    client: &OpenRouterClient,
    settings: &SharedSettings,
    clip: RecordedClip,
) -> Result<()> {
    let config = current_openrouter_config(settings)?;
    let compressed_audio = compress_to_m4a(&clip)?;
    let transcript = client.transcribe_file(&config, &compressed_audio, "m4a")?;

    if transcript.is_empty() {
        return Ok(());
    }

    paste::paste_text(&transcript)?;
    println!("transcribed {} characters", transcript.chars().count());

    Ok(())
}

#[cfg(target_os = "macos")]
fn current_openrouter_config(settings: &SharedSettings) -> Result<OpenRouterConfig> {
    let settings = settings
        .read()
        .map_err(|_| anyhow!("settings lock poisoned"))?
        .clone();

    let api_key = settings.openrouter_api_key.trim().to_owned();
    ensure!(
        !api_key.is_empty(),
        "OpenRouter API key is missing. Click tvoice in the menu bar and add it."
    );

    Ok(OpenRouterConfig {
        api_key,
        model: AppSettings::normalize_model(&settings.openrouter_model),
        prompt: AppSettings::normalize_prompt(&settings.prompt),
        app_title: AppSettings::normalize_app_title(&settings.app_title),
        http_referer: settings.http_referer.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            }
        }),
    })
}

#[cfg(target_os = "macos")]
fn spawn_control_event_tap(control_tx: mpsc::Sender<ControlMessage>) {
    thread::spawn(move || {
        if let Err(error) = install_control_hold_event_tap(control_tx) {
            eprintln!("failed to install the global Control-key event tap: {error:#}");
        }
    });
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
                        ControlMessage::StartRecording {
                            display_uuid: display_uuid_for_point(event.location()),
                        }
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
fn configure_menu_bar_app() {
    unsafe {
        let app = NSApp();
        if app != nil {
            let _ = app.setActivationPolicy_(
                NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory,
            );
        }
    }
}

#[cfg(target_os = "macos")]
struct AppServices {
    _status_item: status_item::StatusItemController,
}

#[cfg(target_os = "macos")]
impl Global for AppServices {}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
enum ControlMessage {
    StartRecording { display_uuid: Option<Uuid> },
    StopRecording,
}
