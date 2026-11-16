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
  # A release app too (what scripts/install.sh installs), and it runs.
  cargo run -q -p karka --bin bundle-karka -- "$CARGO_TARGET_DIR/bundle-release" --release >/dev/null
  "$CARGO_TARGET_DIR/bundle-release/karka.app/Contents/MacOS/karka" --version >/dev/null
  if CARGO=false "$CARGO_TARGET_DIR/debug/bundle-karka" "$out/no-app" --release >/dev/null 2>&1; then
    echo "bundle-karka --release with a failing cargo build did not fail" >&2; exit 1
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
for command in "still x"; do
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
# Size and frame rate, and still frames (a page loaded as a file, so it can use the image next to it).
ffmpeg -loglevel error -y -f lavfi -i color=c=blue:size=64x64 -frames:v 1 "$out/blue.png"
printf '<style>body{margin:0}</style><img id="i" src="blue.png" style="width:100%%;height:100%%"><script type="module">window.__setFrame = () => document.getElementById("i").decode();</script>' > "$out/image.html"
"$KARKA" still "$out/image.html" --frames 5 --frame 3 --width 320 --height 180 --fps 25 --out "$out/r/still.png" --json | grep -q '"width":320'
[[ $(ffmpeg -loglevel error -i "$out/r/still.png" -vf "crop=1:1:160:90" -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | tr -s ' ') == " 0 0 255" ]]
"$KARKA" render "$out/image.html" --frames 3 --width 320 --height 180 --fps 25 --out "$out/r/size.mp4" --json | grep -q '"fps":25'
echo "still and render at 320x180 @ 25 ok"
expect_error render "$out/image.html" --frames 2 --width 321 --out "$out/r/x.mp4"
expect_error render "$out/image.html" --frames 2 --fps 0 --out "$out/r/x.mp4"
expect_error still "$out/image.html" --frames 2 --frame 2 --out "$out/r/x.png"
expect_error still "$out/image.html" --frames 2 --width 10 --out "$out/r/x.png"
expect_error render "$out/image.html" --frames 2 --width 10 --out "$out/r/x.mp4"
# A still of a page that never draws (stall watchdog, ~10 s).
expect_error still "$out/no-set-frame.html" --frames 2 --out "$out/r/x.png"
expect_error still "$out/image.html" --frames 2 --out /dev/null/x.png
mkdir -p "$out/read-only" && cp "$out/image.html" "$out/read-only/" && chmod 555 "$out/read-only"
expect_error still "$out/read-only/image.html" --frames 2 --out "$out/r/x.png"
chmod 755 "$out/read-only"

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
# Projects: make one, edit it with every command, look at it, render it, open the Studio.
P=$out/project
k() { "$KARKA" --json "$@"; }
ok() { "$@" | grep -q '"ok":true' || { echo "failed: $*" >&2; exit 1; }; }
ok k new "$P" --width 640 --height 360 --fps 10 --background "#111"
ok k new "$P" --empty --force
ok k new "$out/phone" --size phone
ffmpeg -loglevel error -y -f lavfi -i testsrc2=size=320x180:rate=10:duration=2 -f lavfi -i sine=duration=2 -shortest "$P/media/clip.mp4"
ffmpeg -loglevel error -y -f lavfi -i color=c=blue:size=64x64 -frames:v 1 "$P/media/logo.png"
ok k add-track "$P" --id V1 --name Video
ok k add-track "$P" --id T1 --index 1
ok k scene "$P" Card --track T1 --duration 10
ok k add "$P" --track V1 type=video src=media/clip.mp4 trim=5
ok k add "$P" --track V1 type=image src=media/logo.png duration=5 fit=contain
ok k add "$P" --track T1 type=text text=2024 duration=5 style.fontSize=40
ok k set "$P" text-1 style.color=red fadeIn=2
ok k set "$P" T1 name=Titles
ok k set "$P" project duration=30
ok k move "$P" text-1 --start 20
ok k split "$P" video-1 --at 5
ok k add "$P" --track T1 type=color color=red duration=2
ok k remove "$P" color-1
ok k show "$P"
ok k validate "$P"
ok k fields
ok k fields video
ok k probe "$P/media/clip.mp4" --fps 10
ok k probe "$P/media/logo.png"
ok k still "$P" --frame 12
ok k still "$P/project.json" --frame 3 --out "$out/r/project-still.png"
ok k render "$P" --progress
ok k render "$P" --software --mode multi --workers 2 --out "$out/r/project-multi.mp4"
echo "project commands ok"
expect_error new "$P"
expect_error scene "$P" lower
expect_error fields gif
expect_error probe "$P/media/missing.mp4"
expect_error add "$P" --track V1 text=x
expect_error add "$P" --track V1 type=text text=x
expect_error add "$P" --track V1 type=color color=red duration=5 start=0
expect_error add "$P" --track V1 broken
expect_error set "$P" text-1 size=3
expect_error move "$P" nope --start 1
expect_error split "$P" video-1 --at 0
expect_error remove "$P" nope
expect_error still "$P" --frame 30
expect_error render "$P" --width 1280
expect_error show "$out/missing-project"
# A scene that does not compile stops the render at once, with the compiler's message.
cp -R "$P" "$out/broken-scene"
printf 'export default () => <div>;\n' > "$out/broken-scene/scenes/Card.tsx"
expect_error still "$out/broken-scene" --frame 12
# A project with problems lists them, as JSON and as text.
cp -R "$P" "$out/bad-project"
sed -i '' 's|media/clip.mp4|media/gone.mp4|' "$out/bad-project/project.json"
expect_error validate "$out/bad-project"
expect_error render "$out/bad-project"
{ "$KARKA" validate "$out/bad-project" 2>&1 || true; } | grep -q "file not found: media/gone.mp4"
printf '{' > "$out/bad-project/project.json"
expect_error still "$out/bad-project"
echo "project errors ok"
# The Studio: serve, answer, stop on request (so the instrumented binary exits and writes its data).
"$KARKA" studio "$P" --port 4719 > "$out/studio.json" 2>"$out/studio.log" &
studio=$!
for _ in $(seq 1 50); do curl -s localhost:4719/api/project >/dev/null && break; sleep 0.2; done
curl -s localhost:4719/api/project | grep -q '"file":"project.json"'
curl -s -X POST localhost:4719/api/quit | grep -q stopping
wait $studio
grep -q stopped "$out/studio.json"
expect_error studio "$out/missing-project" --port 4719
echo "studio ok"

# A relative TMPDIR still gives each process its own CEF profile.
TMPDIR=relative-tmp "$KARKA" smoke --frames 2 --out "$out/e" --json | grep -q '"ok":true'
echo "relative TMPDIR ok"

# Render workers run directly: no job, and a parent that stops listening.
if KARKA_WORKER=1 "$KARKA" </dev/null >/dev/null 2>&1; then echo "worker without a job did not fail" >&2; exit 1; fi
# (The worker is expected to fail, so the pipeline's exit status is ignored.)
printf '1080 1920 1 2 0 1 0\ndata:text/html,%%3Cscript%%3Ewindow.__setFrame%%3D()%%3D%%3E%%7B%%7D%%3C%%2Fscript%%3E' | KARKA_WORKER=1 "$KARKA" 2>/dev/null | true || true
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
# A process killed while writing its data (a Chromium helper at shutdown, a worker stopped on an
# error) leaves a half-written file that would stop the merge; set those aside.
profdata=$(ls "$(rustc --print sysroot)"/lib/rustlib/*/bin/llvm-profdata | head -1)
for f in "$CARGO_TARGET_DIR"/*.profraw; do
  if ! "$profdata" show "$f" >/dev/null 2>&1; then echo "skipping unreadable $(basename "$f")"; rm -f "$f"; fi
done
cargo llvm-cov report --summary-only
if [[ ${1:-} == --html ]]; then
  cargo llvm-cov report --html --output-dir "$CARGO_TARGET_DIR"
  echo "HTML report: $CARGO_TARGET_DIR/html/index.html"
fi
# Every line ran at least once, in a unit test or a real run, and every function (closures too)
# ran. (llvm-cov's own line total counts a function compiled into both the test binary and karka
# once per copy, so it calls a line missed when only one copy ran it; the merged per-line data
# below does not.)
cargo llvm-cov report --lcov --output-path "$CARGO_TARGET_DIR/lcov.info" >/dev/null
missed=$(awk -F'[:,]' '/^SF:/ { file = $2 } /^DA:/ && $3 == 0 { print file ":" $2 }' "$CARGO_TARGET_DIR/lcov.info" | sed "s|$PWD/||")
if [[ -n $missed ]]; then
  echo "lines never run:"; echo "$missed"
  echo "line coverage is below 100%" >&2
  exit 1
fi
cargo llvm-cov report --fail-under-functions 100 >/dev/null || {
  echo "some functions never ran (often an error closure: give it a test, or a form without a closure)" >&2
  exit 1
}
echo "every line and every function ran"
