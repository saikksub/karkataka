#!/usr/bin/env bash
# Install karka for this user, so `karka` works from any folder. Run it from the repo; run it again
# to update after a `git pull`.
#
#   scripts/install.sh
#
# macOS: builds a release karka.app into $KARKA_HOME (default ~/.karka) and links
# $KARKA_BIN/karka (default ~/.local/bin) to the binary inside it. CEF only runs from inside the
# app, so the app stays there and the link points into it.
# Linux: builds a release karka and writes $KARKA_BIN/karka, a wrapper that runs it with CEF's
# libraries from .cef, so keep the repo where it is. (Linux has never run; see the README.)
set -euo pipefail
cd "$(dirname "$0")/.."
home=${KARKA_HOME:-$HOME/.karka}
bin=${KARKA_BIN:-$HOME/.local/bin}

command -v ffmpeg >/dev/null || echo "warning: ffmpeg is not on your PATH; karka needs it to render (brew install ffmpeg)" >&2
./scripts/fetch-cef.sh

mkdir -p "$home" "$bin"
if [[ $(uname -s) == Darwin ]]; then
  cargo run -q -p karka --bin bundle-karka -- "$home" --release
  target=$home/karka.app/Contents/MacOS/karka
  ln -sfn "$target" "$bin/karka"
else
  cargo build --release -p karka
  target=$PWD/target/release/karka
  # The binary finds CEF's libraries through LD_LIBRARY_PATH, so the command is a small wrapper.
  rm -f "$bin/karka"
  printf '#!/bin/sh\nLD_LIBRARY_PATH="%s${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" exec "%s" "$@"\n' "$PWD/.cef" "$target" > "$bin/karka"
  chmod +x "$bin/karka"
fi

"$bin/karka" --version
echo "Installed: $bin/karka -> $target"
case ":$PATH:" in
  *":$bin:"*) ;;
  *) echo "Add $bin to your PATH, for example: echo 'export PATH=\"$bin:\$PATH\"' >> ~/.zshrc && exec zsh" ;;
esac
