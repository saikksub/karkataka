#!/usr/bin/env bash
# Code coverage from the unit tests plus real renders, which are the only way to run the engine's
# browser loop, the worker processes and the encoder. Fails unless every line is covered.
#
#   scripts/coverage.sh          print the summary table
#   scripts/coverage.sh --html   also write target/coverage/html/index.html
#
# Needs cargo-llvm-cov (brew install cargo-llvm-cov) and `rustup component add llvm-tools-preview`.
# Everything builds into target/coverage, so normal builds and target/bundle are left alone.
set -euo pipefail
cd "$(dirname "$0")/.."

export CARGO_TARGET_DIR=target/coverage
eval "$(cargo llvm-cov show-env --sh 2>/dev/null)"
cargo llvm-cov clean --workspace
out=$CARGO_TARGET_DIR/runs
rm -rf "$out" && mkdir -p "$out"

echo "== unit tests"
cargo test --workspace -q 2>&1 | grep -E "^test result" | grep -v " 0 passed" || true

echo "== instrumented build"
if [[ $(uname -s) == Darwin ]]; then
  # bundle-karka builds the instrumented binaries (it runs cargo build in this environment) and
  # puts them into an app of their own.
  cargo run -q -p karka --bin bundle-karka -- "$CARGO_TARGET_DIR/bundle" >/dev/null
  if "$CARGO_TARGET_DIR/debug/bundle-karka" /dev/null/app >/dev/null 2>&1; then
    echo "bundle-karka into an impossible folder did not fail" >&2; exit 1
  fi
  export KARKA=$CARGO_TARGET_DIR/bundle/karka.app/Contents/MacOS/karka
else
  cargo build -q -p karka --bins
  "$CARGO_TARGET_DIR/debug/bundle-karka" >/dev/null
  export KARKA=$CARGO_TARGET_DIR/debug/karka
fi

echo "== renders"
# Every mode (GPU/CPU, one browser/two worker processes), with sync and async frames.
FRAMES=20 tests/scenes/run.sh async webgl

expect_error() {
  local json
  json=$("$KARKA" "$@" --json 2>/dev/null || true)
  if [[ $json == *'"ok":false'* ]]; then echo "error path ok: $*"; else echo "expected an error from: $* (got: $json)" >&2; exit 1; fi
}
printf '<p>no __setFrame here</p>' > "$out/no-set-frame.html"
# Draws frames but no frame-number cells: the frame check fails and the engine must stop.
printf '<script>window.__setFrame = () => {};</script>' > "$out/no-cells.html"
mkdir -p "$out/png-is-a-folder/frame0.png"

# Options.
expect_error smoke --frames 5 --out "$out/e" --mode single --workers 2
expect_error smoke --frames 5 --out "$out/e" --mode multi --workers 0
expect_error smoke --frames 5 --out "$out/e" --page "$out/missing.html"
expect_error smoke --frames 5 --out /dev/null/out
for command in studio "still x"; do
  # shellcheck disable=SC2086
  expect_error $command
done
# Encoder: ffmpeg missing, unknown encoder, an encoder that fails only after reading a frame.
PATH=/usr/bin:/bin expect_error smoke --frames 5 --out "$out/e"
expect_error smoke --frames 5 --out "$out/e" --encoder no-such-encoder
expect_error smoke --frames 1 --out "$out/e" --encoder h263
# Output: frame0.png cannot be written.
expect_error smoke --frames 2 --out "$out/png-is-a-folder"
# Pages that break: no __setFrame (stall watchdog, ~10 s each), no frame-number cells.
expect_error smoke --frames 3 --out "$out/e" --page "$out/no-set-frame.html"
expect_error smoke --frames 4 --out "$out/e" --page "$out/no-set-frame.html" --mode multi --workers 2
expect_error smoke --frames 5 --out "$out/e" --page "$out/no-cells.html"
expect_error smoke --frames 5 --out "$out/e" --page "$out/no-cells.html" --mode multi --workers 2
# karka render: props reach the page intact (it only defines __setFrame when they do).
# A module script, like a built scene: it runs after the config script karka appends.
printf '<script type="module">if (window.__karka?.props.s === "<\\/script>" && window.__karka.frames === 2) window.__setFrame = () => {};</script>' > "$out/props.html"
mkdir -p "$out/r"
"$KARKA" render "$out/props.html" --frames 2 --props '{"s": "</script>"}' --out "$out/r/props.mp4" --json --progress 2>"$out/r/progress.log" | grep -q '"ok":true'
grep -qx '{"frame":2,"frames":2}' "$out/r/progress.log"
echo "render with props and progress ok"
expect_error render "$out/missing.html" --out "$out/r/x.mp4"
expect_error render "$out/props.html" --props '{' --out "$out/r/x.mp4"
expect_error render "$out/props.html" --out /dev/null/x.mp4
expect_error render "$out/props.html" --frames 2 --mode single --workers 2
expect_error render "$out/props.html" --frames 2 --out "$out/r/x.mp4" --encoder no-such-encoder
expect_error render "$out/no-set-frame.html" --frames 1 --out "$out/r/x.mp4"
# Audio: the page reports a clip (and logs something else, which must be left alone), in one
# browser and through worker processes; a report that throws fails the render.
ffmpeg -loglevel error -y -f lavfi -i sine=duration=1 "$out/tone.wav"
printf '<script type="module">console.log("hello"); window.__setFrame = () => {}; window.__karkaReport = () => JSON.stringify({ audio: [{ src: "tone.wav", from: 0, to: null, skip: 0, volume: 1 }] });</script>' > "$out/audio.html"
for mode in "--mode single" "--mode multi --workers 2"; do
  # shellcheck disable=SC2086
  "$KARKA" render "$out/audio.html" --frames 4 $mode --out "$out/r/audio.mp4" --json | grep -q '"audio_clips":1'
  ffprobe -v error -show_entries stream=codec_type -of csv=p=0 "$out/r/audio.mp4" | grep -q audio
  echo "render with audio ok: $mode"
done
printf '<script type="module">window.__setFrame = () => {}; window.__karkaReport = () => { throw new Error("no clips"); };</script>' > "$out/bad-report.html"
expect_error render "$out/bad-report.html" --frames 2 --out "$out/r/x.mp4"
# A relative TMPDIR still gives each process its own CEF profile.
TMPDIR=relative-tmp "$KARKA" smoke --frames 2 --out "$out/e" --json | grep -q '"ok":true'
echo "relative TMPDIR ok"

# Render workers run directly: no job, and a parent that stops listening.
if KARKA_WORKER=1 "$KARKA" </dev/null >/dev/null 2>&1; then echo "worker without a job did not fail" >&2; exit 1; fi
# (The worker is expected to fail, so the pipeline's exit status is ignored.)
printf '1080 1920 2 0 1 0\n<script>window.__setFrame = () => {};</script>' | KARKA_WORKER=1 "$KARKA" 2>/dev/null | true || true
echo "error path ok: worker without a job, worker whose parent stops listening"

# Human-readable output, success and error.
"$KARKA" smoke --frames 5 --out "$out/builtin" >/dev/null
if "$KARKA" smoke --frames 5 --out "$out/e" --page "$out/missing.html" 2>/dev/null; then echo "missing page did not fail" >&2; exit 1; fi
echo "text output ok"

if [[ $(uname -s) == Darwin ]]; then
  # Outside its .app there is no CEF framework.
  KARKA=$CARGO_TARGET_DIR/debug/karka expect_error smoke --frames 2 --out "$out/e"
  # A damaged framework (an empty file, in a copy-on-write clone of the app) cannot load.
  cp -Rc "$CARGO_TARGET_DIR/bundle/karka.app" "$out/damaged.app"
  : > "$out/damaged.app/Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework"
  KARKA=$out/damaged.app/Contents/MacOS/karka expect_error smoke --frames 2 --out "$out/e"
fi

echo "== coverage"
cargo llvm-cov report --summary-only
if [[ ${1:-} == --html ]]; then
  cargo llvm-cov report --html --output-dir "$CARGO_TARGET_DIR"
  echo "HTML report: $CARGO_TARGET_DIR/html/index.html"
fi
cargo llvm-cov report --fail-under-lines 100 >/dev/null || {
  cargo llvm-cov report --show-missing-lines | sed -n '/Uncovered Lines/,$p'
  echo "line coverage is below 100%" >&2
  exit 1
}
echo "every line is covered"
