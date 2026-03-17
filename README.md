# tvoice

`tvoice` is a macOS-only Rust push-to-talk transcriber that runs from the menu bar.

Hold `Control` to record from the default microphone. Releasing `Control` stops the recording, compresses it to `m4a`, sends it to OpenRouter for transcription, and pastes the transcript into the currently focused field. While recording, a small listening indicator appears at the center of the active display.

## Requirements

- macOS
- Rust toolchain
- Permission to use:
  - Microphone
  - Accessibility
  - Input Monitoring

## Configuration

Copy `.env.example` values into your shell environment if you want to seed the initial settings:

```bash
export OPENROUTER_API_KEY=...
export TVOICE_OPENROUTER_MODEL=openai/gpt-audio-mini
```

Optional:

```bash
export TVOICE_APP_TITLE=tvoice
export TVOICE_HTTP_REFERER=https://your-app.example
export TVOICE_OPENROUTER_PROMPT="Transcribe this audio verbatim. Return only the spoken words as plain text."
```

The app stores live settings in `~/Library/Application Support/tvoice/settings.json`. Use the menu-bar item to update the API key, model, and prompt without restarting.

## Run

```bash
cargo run
```

Once running:

1. Click into a text field.
2. Hold `Control` and speak.
3. Release `Control`.
4. Wait for the transcript to be pasted at the current cursor position.

## Notes

- The app shows a `TV` status item in the macOS menu bar. Clicking it opens the settings window.
- This build captures the plain `Control` key globally. While it is running, normal Control-key behavior is intentionally swallowed for push-to-talk.
- Text insertion is implemented by writing the transcript to the clipboard and synthesizing `Command+V`.
- Audio compression uses the built-in macOS `afconvert` utility.
