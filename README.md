# tvoice

Push-to-talk transcription from your system tray. Hold Control, say what you want to type, let go, and it shows up at your cursor.

Rust. macOS and Windows.

## How it works

tvoice runs in your menu bar (macOS) or notification area (Windows). When you hold Control, it records from your default mic and a floating circle appears so you know it's listening. Release Control and the audio goes to OpenRouter for transcription. The transcript gets pasted into whatever field had focus.

You stay in whatever app you were using. Talk, release, keep working.

## Setup

Get an [OpenRouter](https://openrouter.ai/) API key. tvoice asks for it on first launch.

macOS will prompt for permissions:

- Microphone, so it can record
- Accessibility, so it can paste into other apps
- Input Monitoring, so it can catch the Control key globally

Windows only asks about the microphone.

## Configuration

API key, model, and transcription prompt can all be changed from the tray icon. No restart needed. Settings live in your platform's standard app data directory.

Environment variables also work, which is handy for first launch:

| Variable | Purpose |
|---|---|
| `OPENROUTER_API_KEY` | Your API key |
| `TVOICE_OPENROUTER_MODEL` | Model for transcription (default: `openai/gpt-audio-mini`) |
| `TVOICE_OPENROUTER_PROMPT` | Custom prompt sent with your audio |
| `TVOICE_APP_TITLE` | App title in requests |
| `TVOICE_HTTP_REFERER` | HTTP referer for OpenRouter |

## Worth knowing

The bare Control key gets swallowed while tvoice runs. Ctrl+C, Ctrl+V, other combos all work -- it only grabs a solo Control press.

Audio leaves your machine. It goes through OpenRouter to whichever model you configure, so pick one you're okay with.

Pasting works by writing to the clipboard and simulating Cmd+V (macOS) or Ctrl+V (Windows), which means your clipboard contents get overwritten.

macOS compresses audio with `afconvert` before sending. Windows uploads the raw WAV.

## Building from source

Install the Rust toolchain, then:

```
cargo run
```

macOS app bundle:

```
zsh scripts/build_macos_app.sh
open target/debug/tvoice.app
```

Release:

```
zsh scripts/build_macos_app.sh release
open target/release/tvoice.app
```
