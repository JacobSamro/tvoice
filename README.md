# tvoice

`tvoice` is a macOS-only Rust push-to-talk transcriber.

Hold `Control` to record from the default microphone. Releasing `Control` stops the recording, compresses it to `m4a`, sends it to OpenRouter for transcription, and pastes the transcript into the currently focused field.

## Requirements

- macOS
- Rust toolchain
- An `OPENROUTER_API_KEY`
- Permission to use:
  - Microphone
  - Accessibility
  - Input Monitoring

## Configuration

Copy `.env.example` values into your shell environment:

```bash
export OPENROUTER_API_KEY=...
export TVOICE_OPENROUTER_MODEL=openai/gpt-audio-mini
```

Optional:

```bash
export TVOICE_APP_TITLE=tvoice
export TVOICE_HTTP_REFERER=https://your-app.example
```

## Run

```bash
cargo run
```

Once running:

1. Click into a text field.
2. Hold `Control` and speak.
3. Release `Space`.
4. Wait for the transcript to be pasted at the current cursor position.

## Notes

- This MVP captures the plain `Control` key globally. While it is running, normal Control-key behavior is intentionally swallowed for push-to-talk.
- Text insertion is implemented by writing the transcript to the clipboard and synthesizing `Command+V`.
- Audio compression uses the built-in macOS `afconvert` utility.
