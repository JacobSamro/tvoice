#!/bin/zsh

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$PROJECT_ROOT/target/debug/tvoice.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
BIN_PATH="$MACOS_DIR/tvoice"

cargo build --manifest-path "$PROJECT_ROOT/Cargo.toml"

mkdir -p "$MACOS_DIR"
cp "$PROJECT_ROOT/macos/Info.plist" "$CONTENTS_DIR/Info.plist"
cp "$PROJECT_ROOT/target/debug/tvoice" "$BIN_PATH"
chmod +x "$BIN_PATH"

echo "$APP_DIR"
