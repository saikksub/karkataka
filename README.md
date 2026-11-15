# Karkataka

Write video scenes as web pages. Render them to MP4 with a Rust engine that drives Chromium offscreen and encodes on the GPU.

> **Status: early development.** The engine works: it renders any web page frame by frame and encodes it to MP4, about 2x faster than Remotion on our benchmark. `karka render`, `karka still`, the Studio and the TypeScript runtime are not built yet. See [Roadmap](#roadmap).

## Contents

1. [Setup](#setup)
2. [Build and test](#build-and-test)
3. [Benchmark against Remotion](#benchmark-against-remotion)
4. [Using karka](#using-karka)
5. [How it works](#how-it-works)
6. [Use the engine as a library](#use-the-engine-as-a-library)
7. [Project layout](#project-layout)
8. [Roadmap](#roadmap)

## Setup

Do this once. You need Rust (stable), Node.js with pnpm, and FFmpeg.

**1. Install FFmpeg.** Karkataka does not ship it. Every frame is piped into the `ffmpeg` command, so it must be on your `PATH`.

```bash
brew install ffmpeg        # macOS
sudo apt install ffmpeg    # Debian / Ubuntu
```

**2. Get the code and CEF** (Chromium, about 135 MB, version pinned in `cef.lock`):

```bash
git clone https://github.com/saikksub/karkataka
cd karkataka
./scripts/fetch-cef.sh
pnpm install
```

**3. macOS only: create the app bundle.** On macOS, CEF only runs from inside an `.app`. This creates `target/bundle/karka.app`:

```bash
cargo run -p karka --bin bundle-karka
```

**4. Check that it works.** This renders a built-in test page for 300 frames, checks every frame, and writes `out/frame0.png` and `out/smoke.mp4`:

```bash
target/bundle/karka.app/Contents/MacOS/karka smoke --out out    # macOS
cargo run --release -p karka -- smoke --out out                  # Linux
```

## Build and test

Do this after every code change.

**1. Run the unit tests** (a few seconds):

```bash
cargo test
```

**2. Build a release binary.** On macOS, also copy it into the app bundle, because the scene tests and benchmarks run the app. Delete each old file before copying; copying over it makes macOS kill the app on launch (exit code 137).

```bash
cargo build --release -p karka --bins

# macOS only: refresh the app bundle
B=target/bundle/karka.app/Contents
rm -f $B/MacOS/karka && cp target/release/karka $B/MacOS/karka
for h in $B/Frameworks/karka\ Helper*.app; do
  n=$(basename "$h" .app)
  rm -f "$h/Contents/MacOS/$n" && cp target/release/karka_helper "$h/Contents/MacOS/$n"
done
```

**3. Run the scene tests** (about 5 minutes):

```bash
tests/scenes/run.sh
```

This renders 12 animation scenes, each in 4 modes (GPU and CPU painting, each with one browser and with 2 worker processes): 48 renders. It prints one line per render and ends with `N failed`; the exit code is the number of failures. A render fails when:

- any frame does not show its own frame number (a frame is missing, stale or out of order),
- the scene cannot run (no WebGL, a JavaScript error): its frame-number cells turn red,
- the engine sends more than 30 redraw requests per frame (healthy is 1–15).

For a quicker run while working:

```bash
MODES="gpu software" FRAMES=60 tests/scenes/run.sh layout webgl
```

When a render fails, look in `out/scenes/<scene>/<mode>/`: `stderr.log` has the errors, and `frame0.png` and `smoke.mp4` show the picture.

| Scene | Covers |
|---|---|
| `css-3d` | perspective, `preserve-3d`, `backface-visibility` |
| `filters` | `blur`, `drop-shadow`, `hue-rotate`, `backdrop-filter`, `mix-blend-mode` |
| `gradients-masks` | conic and radial gradients, `clip-path`, `mask-image`, border-radius morphing |
| `svg` | per-frame path data, stroke drawing, `feTurbulence`, SMIL animations |
| `canvas2d` | 3000 particles, additive blending, shadows, text |
| `webgl` | a fragment shader |
| `text` | gradient text, variable weight, typewriter, non-Latin scripts, emoji |
| `images` | SVG and PNG images, `object-fit`, tiling, a new image decoded every frame |
| `async` | frames that wait on timers, `fetch`, `requestAnimationFrame` and fonts |
| `css-animations` | `@keyframes` and `element.animate()` |
| `dom-stress` | 2500 elements restyled every frame |
| `layout` | grid and flex reflow, scrolling with sticky headers |

**4. Optional: code coverage** (about 2 minutes). Install the tool once with `brew install cargo-llvm-cov` and `rustup component add llvm-tools-preview`, then:

```bash
scripts/coverage.sh          # summary table
scripts/coverage.sh --html   # also writes target/coverage/html/index.html
```

It runs the unit tests, real renders in every mode, and every error path it can trigger (bad options, missing or failing ffmpeg, broken pages, a damaged CEF framework, workers that fail), using an instrumented build in `target/coverage`, so your normal build and app are not touched. It fails unless every line is covered. Last result: 100% of lines and functions, 99.5% of regions; the uncovered regions are failures the OS or CEF never report here, such as CEF failing to create a browser.

To add a scene, add `tests/scenes/<name>.html`. It defines `window.scene = (n) => { ... }`, which draws frame `n` into `#stage` (it may return a promise), and calls `fail("why")` if it cannot run. The harness (`_head.html`, `_tail.html`) adds the frame-number cells.

## Benchmark against Remotion

`bench/scene.html` is a 300-frame, 1080x1920 scene. A Remotion copy draws the same picture in `~/Test/my-video/src/karka-bench/` (set `REMOTION_DIR` if yours is elsewhere). Build and refresh the app first (step 2 above), then:

```bash
bench/run.sh "karka:--software" "karka:" "remotion:--hardware-acceleration=if-possible"
```

Each setup runs 3 times. Results on an Apple M1 with 8 GB:

| Renderer | Setup | Time |
|---|---|---|
| Karkataka | CPU painting (`--software`) | **4.2–4.3 s** |
| Karkataka | GPU painting | 6.5 s |
| Karkataka | CPU painting, 2 processes (`--mode multi`) | 6.1–6.8 s |
| Remotion 4.0.537 | hardware encoding | 9.6–10.3 s |
| Remotion 4.0.537 | default (libx264) | 11.5–11.8 s |

## Using karka

Only `karka smoke` works today. It renders a page, checks every frame and encodes an MP4.

```bash
karka smoke --page my-scene.html --out out --json
```

| Option | Meaning |
|---|---|
| `--page <file>` | Page to render. It must define `window.__setFrame(n)` and draw the frame-number cells (see `crates/cli/src/smoke.html`). Default: a built-in page. |
| `--frames <n>` | Number of frames (default 300). |
| `--out <dir>` | Where `frame0.png` and `smoke.mp4` go (default `out`). |
| `--software` | Paint on the CPU instead of the GPU. Faster for most scenes; GPU is faster for 3D transforms and filters. |
| `--mode auto\|single\|multi` | `auto` (default) checks CPU cores and free memory and picks the number of browser processes. `single` uses one browser. `multi` uses several, one process each (`--workers N` sets how many). |
| `--encoder <name>` | FFmpeg encoder. Default: `h264_videotoolbox` on macOS, `h264_nvenc` elsewhere. `libx264` encodes on the CPU. |
| `--json` | Print the result or error as JSON. The exit code is 0 on success. |

More processes only help on machines with spare cores and memory. On an 8 GB M1, one is fastest, and `auto` picks one.

## How it works

1. The engine loads the page in an offscreen Chromium (CEF) browser, 1080x1920.
2. For each frame it calls `window.__setFrame(n)` in the page, and waits if that returns a promise.
3. It asks Chromium to paint, and takes the pixels Chromium hands back (`OnPaint`). No screenshots, no PNG per frame.
4. Chromium sometimes paints an older frame. So the engine draws the frame number into a hidden extra pixel row, and only accepts a paint whose row shows the frame it asked for.
5. It pipes the pixels into FFmpeg, which encodes on the hardware encoder.

A page must draw frame `n` the same way every time. To use CSS animations, pause them and seek them to the frame's time:

```js
window.__setFrame = (n) => {
  for (const a of document.getAnimations()) { a.pause(); a.currentTime = n * 1000 / 30; }
};
```

## Use the engine as a library

> Not on crates.io yet. Once published, it will be four crates: `karka` (the CLI), `karkataka-engine` (page in, raw frames out), `karkataka-encode` (frames into FFmpeg) and `karkataka-media` (planned).

```rust
use karkataka_encode::{Encoder, default_h264_encoder};

fn main() -> Result<(), String> {
    // CEF and the render workers relaunch this executable. Handle that first.
    if let Some(code) = karkataka_engine::run_subprocess() {
        std::process::exit(code);
    }

    let html = r#"<div id="n" style="font-size:200px"></div>
<script>window.__setFrame = (n) => { document.getElementById("n").textContent = n; };</script>"#;

    let mut video = Encoder::spawn("out.mp4".as_ref(), 1080, 1920, 30, default_h264_encoder())?;
    // Let the engine pick how many browser processes this machine can run.
    let workers = karkataka_engine::probe::plan(&karkataka_engine::probe::probe(), 300).workers;
    let job = karkataka_engine::Job { html, width: 1080, height: 1920, frames: 300, workers, gpu: true };
    karkataka_engine::render(&job, |_frame, bgra| video.write_frame(bgra))?;
    video.finish()
}
```

Frames arrive in order, as raw BGRA pixels, on a separate thread.

**Building it:** set `CEF_PATH` to a CEF folder from `scripts/fetch-cef.sh`. The `cef` crate's own download is broken right now. On Linux, also add that folder to `LD_LIBRARY_PATH`. On macOS your program must run from an `.app` bundle with a helper binary; copy `crates/cli/src/bin/karka_helper.rs`, `bundle-karka.rs` and the `[package.metadata.cef.bundle]` section of `crates/cli/Cargo.toml`.

**Publishing (maintainers):** bump `version` in the root `Cargo.toml`, then with `CEF_PATH` set run `cargo publish -p` for `karkataka-encode`, `karkataka-media`, `karkataka-engine` and `karka`, in that order.

## Project layout

```
crates/
  engine/        Renders a page frame by frame
    lib.rs         Public API: Job, render, run_subprocess
    browser.rs     One offscreen browser and its frame loop
    page.rs        The page URL and the hidden frame-number row
    worker.rs      Rendering in several processes
    probe.rs       Picks the number of processes for this machine
  encode/        Pipes frames into FFmpeg
  media/         Video decoding and audio mixing (empty for now)
  cli/           The karka command (main.rs: options; smoke.rs: the smoke test)
bench/           Benchmark against Remotion
tests/scenes/    Animation scenes rendered in every mode
scripts/         fetch-cef.sh
next-action-item.md   What we are doing next, and what is blocked
```

## Roadmap

- [x] Engine: render a page frame by frame and encode to MP4 (`karka smoke`)
- [x] Several browser processes, with automatic choice of how many
- [ ] `karka still` and `karka render`
- [ ] TypeScript runtime: `useFrame`, `interpolate`, `Sequence`, `delayRender`
- [ ] Audio
- [ ] Studio: preview, scrubber, props panel, render button
- [ ] `<Video>` input with hardware decoding
- [ ] GPU texture path (`OnAcceleratedPaint`), so frames skip the CPU copy

**Known limits:** NVENC (`h264_nvenc`, the Linux default) and the whole Linux build have not run yet. Each CEF upgrade can break frame capture, so the CEF version is pinned.

## License

[MIT](LICENSE). You can use, change and share it for free, including in commercial products. Videos you render with Karkataka are yours.

## Name

Karkataka is the Telugu and Kannada name for the zodiac sign Cancer, the crab. The crab points to Ferris, the Rust mascot.

---

Built by [Xploray](https://xploray.com).
