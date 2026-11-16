# Karkataka

Video editing from the command line, for people and AI agents. A video is a **project folder**:
- `project.json` holds the tracks and clips (video, images, audio, text, colors, and your scenes);
- `scenes/*.tsx` are SolidJS components you write in any code editor.

`karka` makes projects, edits them, checks them, shows single frames, and renders MP4s with sound. It also serves the **Studio**, a browser editor with a preview and a timeline.

`karka` is one Rust binary. It renders with offscreen Chromium and encodes on the GPU, about 2x faster than Remotion on our benchmark. Your scenes compile inside that Chromium, so using karka needs no Node.js, npm or build step. You need FFmpeg.

> **Status: early development.** Projects, scenes, stills, rendering (any size and frame rate, with sound) and the Studio work on macOS. Linux has never run. The crate is [on crates.io](https://crates.io/crates/karka) for [use as a library](#use-the-engine-as-a-library), but to use the `karka` command, install it from source with `scripts/install.sh` (below): `cargo install karka` gives a binary that cannot start Chromium on macOS, because CEF only runs from inside an `.app`. See [Roadmap](#roadmap).

## Getting started

On macOS, with [Rust](https://rustup.rs) and [Homebrew](https://brew.sh) installed. Installing takes a few minutes, most of it downloading Chromium (about 135 MB) and building:

```bash
brew install ffmpeg
git clone https://github.com/saikksub/karkataka && cd karkataka
scripts/install.sh                            # installs the karka command for your user

karka new ~/my-video                          # a 1920x1080 project with one scene (--size phone for 9:16)
karka render ~/my-video                       # ~/my-video/out/my-video.mp4, with a progress bar
karka render ~/my-video --scale 2             # the same video in 4K
karka studio ~/my-video                       # edit it at http://127.0.0.1:4000/
```

If the installer says so, add `~/.local/bin` to your PATH first.

Then edit `scenes/Hello.tsx` in your code editor and watch the Studio update. [Setup](#setup) explains each step, and [Quick start](#quick-start) shows what to do next.

## Contents

1. [Setup](#setup)
2. [Quick start](#quick-start)
3. [Projects](#projects)
4. [Scenes](#scenes)
5. [The karka command](#the-karka-command)
6. [Studio](#studio)
7. [Build and test](#build-and-test)
8. [Benchmark against Remotion](#benchmark-against-remotion)
9. [How it works](#how-it-works)
10. [Use the engine as a library](#use-the-engine-as-a-library)
11. [Project layout](#project-layout)
12. [Roadmap](#roadmap)

## Setup

Do this once. You need Rust (stable) and FFmpeg. Node.js and pnpm are needed only to work on Karkataka itself (see [Build and test](#build-and-test)).

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
```

**3. Install the `karka` command**, so it works from any folder:

```bash
scripts/install.sh
```

On macOS this builds a release `karka.app` into `~/.karka` (CEF only runs from inside an `.app`) and links `~/.local/bin/karka` to the binary inside it. On Linux it builds a release `karka` and writes `~/.local/bin/karka`, a small script that runs it with CEF's libraries. Set `KARKA_HOME` or `KARKA_BIN` to install somewhere else. If `~/.local/bin` is not on your PATH, the installer prints the line to add to `~/.zshrc`. Run it again after a `git pull` to update.

**4. Check that it works.** This renders a built-in test page for 300 frames, checks every frame, and writes `out/frame0.png` and `out/smoke.mp4`:

```bash
karka smoke --out out
```

## Quick start

```bash
karka new my-video                 # project.json, scenes/Hello.tsx, media/, types for your editor
cd my-video
karka still . --frame 30           # look at frame 30: out/my-video-30.png
karka render .                     # out/my-video.mp4
karka studio .                     # open http://127.0.0.1:4000/ in a browser
```

Open the folder in your editor (VS Code, Zed, Cursor, ...). `scenes/Hello.tsx` is a SolidJS component: change it, and the Studio's preview updates. `karka new` also writes a `tsconfig.json` and type declarations in `.karka/types/`, so the editor completes and checks scene code without an `npm install`.

Add more:

```bash
karka scene . Intro                                     # scenes/Intro.tsx and a clip after Hello
karka add-track . --id V0 --index 0                     # a track under the others
karka add . --track V0 type=video src=media/clip.mp4    # your footage; no duration: the rest of the file
karka add-track . --id T1                               # a track on top
karka add . --track T1 type=text text="Hello" start=30 duration=60 style.fontSize=96 fadeIn=10
karka validate .
```

Every command takes `--json` and then prints one JSON object, which is how agents use karka. A mistake is refused with the reason, and the file is left alone:

```json
{"ok":false,"error":"tracks[1] clips \"image-1\" (frames 20–59) and \"color-1\" (from 50) overlap; move one or put it on another track","problems":[...]}
```

Examples: `examples/edit/` is a 16:9 edit of footage, a logo, a title and music (`./make-media.sh` makes its media). `examples/c-quiz/` is a 9:16 quiz short, made of one scene clip.

## Projects

A project folder holds `project.json`; its paths are relative to that folder. Times are in frames at the project's fps.

```json
{
  "version": 1,
  "width": 1920, "height": 1080, "fps": 30,
  "duration": 300,
  "background": "#000",
  "tracks": [
    { "id": "V1", "name": "Video", "clips": [
      { "id": "video-1", "type": "video", "src": "media/clip.mp4", "start": 0, "duration": 120, "trim": 30, "volume": 1 }
    ] },
    { "id": "T1", "hidden": false, "muted": false, "clips": [
      { "id": "intro", "type": "component", "component": "scenes/Intro.tsx", "start": 0, "duration": 90, "props": { "title": "Hello" } }
    ] }
  ]
}
```

- **Size and frame rate:** `width` and `height` are even, and the width is at least 72 px; `fps` is 1 to 240. Any aspect ratio works.
- **Length:** `duration` in frames. Leave it out to end where the last clip ends. At most 65536 frames per render.
- **Tracks:** track 0 is the bottom layer, and later tracks draw over it. Clips on one track must not overlap: put overlapping clips on another track. `hidden` hides a track's pictures, and `muted` silences its sound.

**Clips.** Every clip has an `id` (unique; made for you when you leave it out), a `type`, `start` (first frame) and `duration` (frames). Pictures also take:
- `box`: `{x, y, width, height}` in px; the default is the whole frame;
- `opacity`, from 0 to 1;
- `fadeIn` and `fadeOut`, in frames.

| Type | Fields | Notes |
|---|---|---|
| `video` | `src`, `trim`, `volume`, `fit` | `trim` skips that many source frames. Its sound plays unless the track is muted. Any format FFmpeg reads. |
| `image` | `src`, `fit` | `fit` is `cover` (default), `contain` or `fill`. |
| `audio` | `src`, `trim`, `volume` | Draws nothing. |
| `text` | `text`, `style` | `style` takes `fontSize`, `fontFamily`, `fontWeight`, `color`, `background`, `align`, `verticalAlign`, `padding` and `lineHeight`. |
| `color` | `color` | Any CSS color or gradient. |
| `component` | `component`, `props` | A scene file and its props. See [Scenes](#scenes). |

`karka fields` prints this list as JSON. `karka validate` lists every problem with its place, for example `tracks[0].clips[2] ("video-1").src file not found: ...`. Edits that would add a problem are refused.

## Scenes

A scene is a SolidJS component in a `.tsx` file: anything a web page can draw, animated frame by frame. A `component` clip shows it, with the clip's `props`, inside the clip's box, for the clip's duration. `karka scene <project> <Name>` writes one from a template.

```tsx
import { interpolate, useDuration, useFrame } from "@karkataka/runtime";

/** Optional: how the Studio edits the props. */
export const schema = { title: { type: "string", title: "Title" } };

export default function Title(props: { title: string }) {
  const f = useFrame(); // 0 at the clip's start
  const length = useDuration(); // the clip's length in frames
  return <h1 style={{ opacity: `${interpolate(f(), [0, 15, length - 15, length], [0, 1, 1, 0])}` }}>{props.title}</h1>;
}
```

- **Imports:** a scene can import `solid-js`, `solid-js/web`, `solid-js/store`, `@karkataka/runtime`, and files in the project:
  - other `.ts`/`.tsx`/`.js`/`.json` files are compiled with it;
  - any other file imports as its URL (`import logo from "../media/logo.png"`).

  Other npm packages are not available: karka compiles scenes itself, without a bundler.
- **Only what is used is compiled:** the scene files on the timeline and what they import. Other code in the folder (tests, tool configs) is left alone.
- **Errors:** a scene that does not compile, or that throws, stops `still` and `render` at once, with the compiler's message, file and line.
- **The same picture every time:** draw only from the frame and the props. No `Date.now()`, `Math.random()` or timers.

| `@karkataka/runtime` | What it does |
|---|---|
| `useFrame()` | The current frame, as a Solid accessor, counted from the start of the clip or enclosing `Sequence`. |
| `useDuration()` | The length of the clip or enclosing `Sequence`, in frames. |
| `<Sequence from duration>` | Shows its children from frame `from` for `duration` frames. Inside it, `useFrame()` starts at 0. Sequences nest. |
| `interpolate(x, [0, 10], [0, 1], { easing, clamp })` | Piecewise linear mapping. It holds the end values unless `clamp: false`. |
| `spring({ frame, fps, from, to, stiffness, damping, mass })` | A spring released at rest at frame 0. It is computed exactly, so any frame renders on its own. Before frame 0 it stays at `from`, so `frame: f() - 20` delays it 20 frames. |
| `<Audio src start trim volume>` | Plays a sound from frame `start` until the clip or Sequence ends. `trim={[a, b]}` plays source frames `a` to `b`. A relative `src` is a file in the project folder. |
| `waitFor(promise)` | Makes the current frame wait for an image, font or data before it is captured. |
| `config` | `fps`, `width`, `height` and `frames` of the video. |
| `<SafeArea>` | For 9:16 phone video: the Instagram Reels safe zone. Put what the viewer must read inside it. |

## The karka command

Paths can be a project folder or its `.json` file. Add `--json` to any command to get `{"ok": true, "result": ...}` or `{"ok": false, "error": "...", "problems"?: [...]}`; the exit code is 1 on failure.

| Command | What it does |
|---|---|
| `new <dir> [--size \| --width --height] [--fps --background] [--empty] [--force]` | A project folder: `project.json` with a starter scene clip (none with `--empty`), `scenes/Hello.tsx`, `media/`, and editor types. `--size` is `landscape` (1920x1080, the default), `phone` (1080x1920), `square` (1080x1080) or `portrait` (1080x1350); `--width` and `--height` set any other size. 30 fps by default. In a 9:16 project, new scenes put their text inside `<SafeArea>`. |
| `scene <project> <Name> [--track] [--duration]` | `scenes/<Name>.tsx` from a template (kept if it exists) and a clip that shows it, after the track's last clip. |
| `show <project>` | The project, its length and its problems. |
| `validate <project>` | Fails with every problem, if there are any. |
| `fields [type]` | The fields of each clip type and of the project. |
| `probe <media> [--fps 30]` | Length (in seconds, and in frames at that fps), size, frame rate, and whether there is video, audio or a still image. |
| `add-track <project> [--id] [--name] [--index]` | A new track, on top unless `--index` says where. |
| `add <project> --track <id> type=<type> key=value...` | A new clip. With no `start`, it goes after the track's last clip. A video or audio clip with no `duration` gets the rest of the file. |
| `set <project> <clip\|track\|project> key=value...` | Change fields. Objects (`style`, `box`, `props`) are merged; `key=null` removes a field. |
| `move <project> <clip> [--start <frame>] [--track <id>]` | Move a clip in time, to another track, or both. |
| `split <project> <clip> --at <frame>` | Cut a clip in two. The second part carries on in the source. |
| `remove <project> <clip\|track>` | Remove a clip, or a track with its clips. |
| `still <project> [--frame 0] [--out] [--scale 1]` | One frame as PNG (default `out/<name>-<frame>.png` in the project). |
| `render <project> [--out] [--scale 1] [--progress]` | The MP4, with sound (default `out/<name>.mp4`). In a terminal it shows what it is doing and a progress bar (frames, elapsed time, speed, time left) on stderr. `--progress` prints `{"frame": n, "frames": total}` lines to stderr instead, for programs. The result includes `started_at` and `finished_at` (UTC, ISO 8601) and `elapsed_secs` for the whole command. |
| `studio <project> [--port 4000]` | The Studio, at `http://127.0.0.1:4000/`. |
| `smoke [--page] [--frames] [--out]` | The engine self-test: renders a 1080x1920 page, checks every frame, and writes `frame0.png` and `smoke.mp4`. |

Values in `key=value` are JSON when they parse as JSON (`30`, `true`, `null`, `{"x":0}`), and text otherwise. A text field keeps what was typed, so `text=2024` stays text. Dotted keys set nested fields: `style.fontSize=96`, `props.title="..."`.

**Bigger output with `--scale`.** `still` and `render` take `--scale 2` (up to 4), which renders the project at 2x its size: a 1920x1080 project becomes a 3840x2160 (4K) video, and a 1080x1920 one becomes 2160x3840. The layout stays the same; text, shapes and CSS are drawn sharp at the bigger size, while images and video clips are scaled up. To make a project 4K for good instead, set its size: `karka set . project width=3840 height=2160`, and size the scenes for it.

`still`, `render` and `smoke` take `--software`, which paints on the CPU (faster for most videos; the GPU is faster for 3D transforms and filters). `render` and `smoke` also take:
- `--mode auto|single|multi`: `auto` (default) picks the number of browser processes from the CPU cores and free memory;
- `--workers N`: the number of processes in `multi` mode;
- `--encoder <name>`: the FFmpeg encoder. The default is `h264_videotoolbox` on macOS and `h264_nvenc` elsewhere; `libx264` encodes on the CPU.

**Plain web pages.** `still` and `render` also take an `.html` page instead of a project, with `--frames`, `--props <json>`, `--width`, `--height` and `--fps`. karka adds `window.__karka = {props, frames, fps, width, height}` to the page. The page must define `window.__setFrame(n)`, which draws frame `n` and returns a promise if it has to wait.

## Studio

`karka studio <project>` serves the Studio for a project from the binary. Open the address it prints. It edits the same `project.json`, through the same rules as the CLI.

- **Preview.** It draws the project with the same player that renders it, and compiles your scenes the same way. Save a scene file in your editor and the preview reloads, staying on the same frame. The preview seeks the video files; a render uses exact frames cut by FFmpeg.
- **Timeline.**
  - Tracks are shown top layer first, with hide (👁) and mute (♪) per track.
  - Click or drag the ruler to seek. Click a clip to select it, and drag it to move it.
  - Play/pause (Space), loop, step one or ten frames (← →, Shift), first/last frame (Home/End).
  - Split the selected clip at the playhead (S) or delete it (⌫).
  - Undo and redo (⌘Z, ⇧⌘Z).
  - "Safe zone" (9:16 projects) outlines the Instagram Reels safe zone.
- **Inspector.**
  - With a clip selected: its fields. A scene's props get their own fields when it exports a `schema` (JSON Schema keywords: `type`, `title`, `description`, `enum`, `minimum`/`maximum`, plus `multiline`).
  - With nothing selected: the project's settings, and buttons to add a track or a clip at the playhead.
  - A refused edit says why. The project's problems (a missing file) are listed at the top.
- **Live.** When something else changes `project.json`, such as an agent running `karka`, the Studio shows the change at once.
- **Render.** The button renders with a progress bar, then plays `out/<name>.mp4`.

`POST /api/quit` stops the Studio (as Ctrl+C does), for scripts.

## Build and test

Do this after every code change. Working on Karkataka needs Node.js and pnpm as well: run `pnpm install` once.

**1. Run the unit tests** (a few seconds):

```bash
cargo test
pnpm test        # runtime, compiler, Studio and the c-quiz scene (Vitest)
pnpm typecheck
```

**JavaScript built into karka.** The binary carries five pieces of JavaScript, built from `packages/`:
- the compiler (Babel with the Solid preset);
- solid-js;
- the runtime;
- the player;
- the Studio.

After changing any of them, rebuild with `pnpm assets` and commit `crates/karka/assets/`. That is what lets `cargo install` work without Node.

**2. Build a release app** (macOS), which the scene tests and benchmarks run. It replaces `target/bundle/karka.app`:

```bash
cargo run -p karka --bin bundle-karka -- --release    # Linux: cargo build --release -p karka
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

It runs the unit tests, real renders in every mode, and every error path it can trigger (bad options, missing or failing ffmpeg, broken pages, a damaged CEF framework, workers that fail), using an instrumented build in `target/coverage`, so your normal build and app are not touched. It fails unless every line ran at least once (in a unit test or a real run) and every function ran, closures included. The real runs cover projects (every command, a scene that does not compile, a project with problems) and a `karka studio` session.

**5. End-to-end tests** (about a minute). `edit.sh` works as an agent would. It builds a 16:9 edit from media using only `karka` commands, and checks that a mistake is refused. Then it looks at stills and renders. Its media are solid colours, so pixel checks prove frame-exact trimming, splitting, layering, boxes and the title. It also checks the size, length and sound of the MP4. `c-quiz.sh` renders the quiz project (its scene compiled inside karka's Chromium) with the GPU and with `--software`, and checks that each of its 11 sounds starts within 30 ms of its time.

```bash
tests/examples/edit.sh
tests/examples/c-quiz.sh
```

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

## How it works

For `still` and `render`, karka first checks the project, and cuts each video clip into JPEG frames with FFmpeg (cached in `.karka/frames/`). It then writes one HTML page. That page holds:
- the compiler, solid-js, the runtime and the player;
- the project;
- the project's scene files as source text.

In the browser, the page compiles the scenes used by the project with Babel and points an import map at the compiled modules. Then the engine:

1. Loads the page in an offscreen Chromium (CEF) browser, at the project's size, and waits for the scenes to compile. A compile error stops the render with the compiler's message.
2. For each frame, calls `window.__setFrame(n)` in the page, and waits if that returns a promise.
3. Asks Chromium to paint, and takes the pixels Chromium hands back (`OnPaint`). No screenshots, no PNG per frame.
4. Chromium sometimes paints an older frame. So the engine draws the frame number into a hidden extra pixel row, and only accepts a paint whose row shows the frame it asked for.
5. Pipes the pixels into FFmpeg, which encodes on the hardware encoder.
6. Before frame 0, the page lists its audio clips: video and audio clips, and `<Audio>` in scenes. After the frames, a second FFmpeg run trims, delays and mixes them and adds them to the file. The video stream is copied, not encoded again.

The player draws a video clip's frame as the matching JPEG, and waits until it has decoded. Every frame is therefore exact, and any format FFmpeg reads works, even ones Chromium cannot play.

## Use the engine as a library

Add it with `cargo add karka` (see **Building it** below for CEF). It is one crate, [`karka`](https://crates.io/crates/karka), with the `karka` binary and these modules: `engine` (page in, raw frames out), `encode` (frames into FFmpeg), `audio` (mixing), `project` (the project format, its checks and edits), and `render`.

```rust
use karka::encode::{Encoder, default_h264_encoder};

fn main() -> Result<(), String> {
    // CEF and the render workers relaunch this executable. Handle that first.
    if let Some(code) = karka::engine::run_subprocess() {
        std::process::exit(code);
    }

    let html = r#"<div id="n" style="font-size:200px"></div>
<script>window.__setFrame = (n) => { document.getElementById("n").textContent = n; };</script>"#;
    // A page with local files (images, video frames) is loaded with karka::engine::file_url instead.
    let url = karka::engine::data_url(html);

    let mut video = Encoder::spawn("out.mp4".as_ref(), 1080, 1920, 30, default_h264_encoder())?;
    // Let the engine pick how many browser processes this machine can run.
    let workers = karka::engine::probe::plan(&karka::engine::probe::probe(), 300).workers;
    let job = karka::engine::Job { url: &url, width: 1080, height: 1920, scale: 1, frames: 300, workers, gpu: true };
    karka::engine::render(&job, |_frame, bgra| video.write_frame(bgra))?;
    video.finish()
}
```

Frames arrive in order, as raw BGRA pixels, on a separate thread. `render` returns the page's audio report, if it made one. `karka::engine::render_frame(&job, n)` renders only frame `n`.

**Building it:** set `CEF_PATH` to a CEF folder from `scripts/fetch-cef.sh`. The `cef` crate's own download is broken right now. On Linux, also add that folder to `LD_LIBRARY_PATH`. On macOS your program must run from an `.app` bundle with a helper binary; copy `crates/karka/src/bin/karka_helper.rs`, `bundle-karka.rs` and the `[package.metadata.cef.bundle]` section of `crates/karka/Cargo.toml`.

**Publishing (maintainers):** run `pnpm assets` and commit the result, bump `version` in the root `Cargo.toml`, then with `CEF_PATH` set run `cargo publish -p karka`.

## Project layout

```
crates/karka/            The karka crate: one library and the karka binary
  src/engine/              Renders a page frame by frame (browser, page, worker, probe)
  src/encode.rs            Pipes frames into FFmpeg
  src/audio.rs             Mixes audio clips into the video
  src/project/             The project format (model), edits (ops), media (probe, frames), its page (page)
  src/render.rs            Renders a page to MP4 or PNG
  src/files.rs             File access with clear errors
  src/cli/                 The commands: mod.rs (options), project.rs, new.rs, types.rs, studio.rs, smoke.rs
  assets/                  JavaScript built into the binary (pnpm assets)
packages/                For the built-in JavaScript (needs Node only to build)
  runtime/                 @karkataka/runtime: useFrame, Sequence, Audio, waitFor, interpolate, spring, SafeArea
  player/                  The SolidJS app that draws a project
  compiler/                Compiles scenes in the page (Babel + Solid), and build-assets.ts
  studio/                  The Studio app
examples/
  edit/                    A 16:9 edit: footage, logo, title, music (make-media.sh makes the media)
  c-quiz/                  A 9:16 quiz short: one scene clip (scenes/Quiz.tsx)
tests/scenes/            Animation pages rendered in every mode
tests/examples/          End-to-end tests: edit.sh (an agent's workflow) and c-quiz.sh
bench/                   Benchmark against Remotion
scripts/                 install.sh, fetch-cef.sh, coverage.sh
next-action-item.md      What we are doing next, and what is blocked
```

## Roadmap

- [x] Engine: render a page frame by frame and encode to MP4, in one or several browser processes
- [x] Projects: tracks and clips (video, image, audio, text, color, scene), any size and frame rate
- [x] One `karka` binary for people and agents: new, scene, edit, validate, probe, still, render, studio; JSON output
- [x] Scenes in SolidJS, compiled inside karka (no Node), with editor types
- [x] Audio mixed into the MP4: video sound, audio clips, `<Audio>` in scenes
- [x] Studio: preview, multi-track timeline with drag, split and undo, inspector, live reload of scenes and of agent edits, render with progress
- [x] On crates.io as `karka` (library)
- [ ] Install without building: a prebuilt `karka.app` with CEF inside, then Homebrew
- [ ] Keyframes: values that change over a clip (position, scale, volume) beyond fades
- [ ] Transitions between clips, speed changes, trimming by dragging clip edges in the Studio
- [ ] npm packages in scenes (karka compiles scenes without a bundler today)
- [ ] GPU texture path (`OnAcceleratedPaint`), so frames skip the CPU copy

**Known limits:**
- Studio previews have no sound.
- Video clips are cut into JPEG frames before a render. This takes disk space (about 0.1–0.3 MB per 1080p frame, cached in `.karka/`).
- NVENC (`h264_nvenc`, the Linux default) and the whole Linux build have not run yet.
- Each CEF upgrade can break frame capture, so the CEF version is pinned.

## License

[MIT](LICENSE). You can use, change and share it for free, including in commercial products. Videos you render with Karkataka are yours.

## Name

Karkataka is the Telugu and Kannada name for the zodiac sign Cancer, the crab. The crab points to Ferris, the Rust mascot.

---

Built by [Xploray](https://xploray.com).
