use anyhow::{Context, Result, anyhow};
use arboard::Clipboard;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, KeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::thread;
use std::time::Duration;

pub fn paste_text(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new().context("failed to access the system clipboard")?;
    let original_text = clipboard.get_text().ok();

    clipboard
        .set_text(text.to_owned())
        .context("failed to write transcript to the clipboard")?;

    send_command_v()?;

    thread::sleep(Duration::from_millis(250));

    if let Some(original_text) = original_text {
        let _ = clipboard.set_text(original_text);
    }

    Ok(())
}

fn send_command_v() -> Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| anyhow!("failed to create keyboard event source"))?;

    let key_down = CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_V, true)
        .map_err(|_| anyhow!("failed to create key down event"))?;
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);

    let key_up = CGEvent::new_keyboard_event(source, KeyCode::ANSI_V, false)
        .map_err(|_| anyhow!("failed to create key up event"))?;
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);

    key_down.post(CGEventTapLocation::HID);
    key_up.post(CGEventTapLocation::HID);

    Ok(())
}
