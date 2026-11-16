# Karkataka

Video editing software that AI agents control. A video is a **project file**: JSON with tracks and clips (video, images, audio, text, colors, and your own SolidJS components). Agents edit it with the `karkataka` command or the TypeScript SDK, look at single frames to check their work, and render MP4s. People open the same file in the **Studio**, a browser editor with a preview and a timeline. A Rust engine renders it: it drives Chromium offscreen and encodes on the GPU, about 2x faster than Remotion on our benchmark.

> **Status: early development.** Editing, checking, stills, rendering (any size and frame rate, with sound) and the Studio work on macOS. Linux has never run. See [Roadmap](#roadmap).

## Contents

1. [Setup](#setup)
2. [Make a video](#make-a-video)
3. [Projects](#projects)
4. [The karkataka command](#the-karkataka-command)
5. [TypeScript SDK](#typescript-sdk)
6. [Component clips](#component-clips)
7. [Studio](#studio)
8. [Build and test](#build-and-test)
9. [Benchmark against Remotion](#benchmark-against-remotion)
10. [The karka engine command](#the-karka-engine-command)
11. [How it works](#how-it-works)
12. [Use the engine as a library](#use-the-engine-as-a-library)
13. [Project layout](#project-layout)
14. [Roadmap](#roadmap)

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

## Make a video

After [Setup](#setup). Tell the commands where karka is (on macOS, inside the app bundle):

```bash
export KARKA=$PWD/target/bundle/karka.app/Contents/MacOS/karka
alias karkataka=$PWD/node_modules/.bin/karkataka
examples/edit/make-media.sh out/demo/media      # a test clip, a logo and music
```

Build a 16:9 edit, check it, look at it, render it:

```bash
cd out/demo
karkataka new demo.json                                   # 1920x1080 at 30 fps
karkataka add-track demo.json --id V1                     # track 0: the bottom layer
karkataka add-track demo.json --id V2                     # draws over V1
karkataka add-track demo.json --id A1
karkataka add demo.json --track V1 type=video src=media/clip.mp4 trim=30     # no duration: the rest of the file
karkataka split demo.json video-1 --at 40                 # cut it in two at frame 40
karkataka add demo.json --track V2 type=image src=media/logo.png start=20 duration=40 \
  box.x=1500 box.y=60 box.width=360 box.height=360 fit=contain
karkataka add demo.json --track V2 type=text text="The title" start=60 duration=30 style.fontSize=120 fadeIn=10
karkataka add demo.json --track A1 type=audio src=media/music.wav duration=90 volume=0.5
karkataka validate demo.json
karkataka still demo.json --frame 50 --out frame50.png    # look at one frame
karkataka render demo.json --out demo.mp4                 # progress: JSON lines on stderr
```

Every command prints one JSON object, so an agent reads the result directly. A mistake is refused with the reason, and the file is left alone:

```json
{ "ok": false, "error": "tracks[1] clips \"image-1\" (frames 20–59) and \"color-1\" (from 50) overlap; move one or put it on another track", "problems": [...] }
```

`examples/edit/edit.json` is this project; `examples/c-quiz/quiz.json` is a 9:16 quiz short made of one component clip.

## Projects

A project file is JSON. Times are in frames at the project's fps. Paths are relative to the project file.

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
      { "id": "text-1", "type": "text", "text": "Hello", "start": 30, "duration": 60, "style": { "fontSize": 96 }, "fadeIn": 10 }
    ] }
  ]
}
```

- **Size and frame rate:** `width` and `height` are even and at least 72 px wide; `fps` is 1 to 240. Any aspect ratio works.
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
| `component` | `component`, `props` | A SolidJS component file and its props. See [Component clips](#component-clips). |

`karkataka fields` prints this list as JSON, for agents. `karkataka validate` lists every problem with its place (`tracks[0].clips[2] ("video-1").src file not found: ...`). Edits that would add a problem are refused.

## The karkataka command

`karkataka` is the agent's interface. Every command prints `{"ok": true, "result": ...}` or `{"ok": false, "error": "...", "problems"?: [...]}`, and exits with 1 on failure. In this repo it is `node_modules/.bin/karkataka`.

| Command | What it does |
|---|---|
| `new <p.json> [--width --height --fps --background] [--force]` | A new, empty project. The default is 1920x1080 at 30 fps. |
| `show <p.json>` | The project, its length and its problems. |
| `validate <p.json>` | Fails with every problem, if there are any. |
| `fields [type]` | The fields of each clip type and of the project. |
| `probe <media> [--fps 30]` | Length (in seconds, and in frames at that fps), size, frame rate, and whether there is video, audio or a still image. |
| `add-track <p.json> [--id] [--name] [--index]` | A new track, on top unless `--index` says where. |
| `add <p.json> --track <id> type=<type> key=value...` | A new clip. With no `start`, it goes after the track's last clip. A video or audio clip with no `duration` gets the rest of the file. |
| `set <p.json> <clip\|track\|project> key=value...` | Change fields. Objects (`style`, `box`, `props`) are merged; `key=null` removes a field. |
| `move <p.json> <clip> [--start <frame>] [--track <id>]` | Move a clip in time, to another track, or both. |
| `split <p.json> <clip> --at <frame>` | Cut a clip in two. The second part carries on in the source. |
| `remove <p.json> <clip\|track>` | Remove a clip, or a track with its clips. |
| `still <p.json> [--frame 0] [--out out/still.png] [--software]` | One frame as PNG: how an agent looks at its work. |
| `render <p.json> [--out out/video.mp4] [--software]` | The MP4, with sound. Progress goes to stderr as `{"frame": n, "frames": total}` lines. |

Values in `key=value` are JSON when they parse as JSON (`30`, `true`, `null`, `{"x":0}`), and text otherwise. A text field keeps what was typed, so `text=2024` stays text. Dotted keys set nested fields: `style.fontSize=96`, `props.question="..."`.

`still` and `render` need `KARKA`, the karka binary, unless `karka` is on your `PATH`, and FFmpeg on your `PATH`. Video clips are cut into frames by FFmpeg first and cached in `.karka/` next to the project.

## TypeScript SDK

`@karkataka/sdk` is what the command uses. Edits are functions that return a new project, so the old one is kept for undo, and they throw an `EditError` saying why when the edit would break the project.

```ts
import { addClip, addTrack, createProject, renderProject, saveProject, splitClip, stillProject } from "@karkataka/sdk";

let project = createProject({ width: 1080, height: 1920, fps: 30 });
project = addTrack(project, { id: "V1" }).project;
project = addClip(project, "V1", { type: "video", src: "media/clip.mp4", duration: 90, trim: 30 }).project;
project = splitClip(project, "video-1", 60).project;
await saveProject("short.json", project);
await stillProject("short.json", 75, "out/check.png");
await renderProject("short.json", "out/short.mp4", { onProgress: (frame, frames) => console.log(frame, frames) });
```

The SDK provides:
- **Model:** `Project`, `Clip` and the other types, `validate`, `durationOf`, `CLIP_FIELDS` and `PROJECT_FIELDS`.
- **Edits:** `createProject`, `updateProject`, `addTrack`, `updateTrack`, `addClip`, `updateClip`, `moveClip`, `splitClip`, `remove` and `find`.
- **Files and media:** `loadProject`, `saveProject` and `probe`.
- **Rendering:** `stillProject` and `renderProject`.

`@karkataka/sdk/edit` has only the model and the edits (no Node or files), for browser code; the Studio uses it.

## Component clips

A component clip draws a SolidJS component: anything a web page can draw, animated frame by frame. `component` is the module's path; its default export takes the clip's `props`. It may also export a `schema` for its props, which the Studio uses to build their fields (see [Studio](#studio)).

```tsx
import { interpolate, useDuration, useFrame } from "@karkataka/runtime";

export const schema = { title: { type: "string", title: "Title" } };

export default function Title(props: { title: string }) {
  const f = useFrame();                 // 0 at the clip's start
  const length = useDuration();         // the clip's duration
  return <h1 style={{ opacity: `${interpolate(f(), [0, 15, length - 15, length], [0, 1, 1, 0])}` }}>{props.title}</h1>;
}
```

| `@karkataka/runtime` | What it does |
|---|---|
| `useFrame()` | The current frame, as a Solid accessor, counted from the start of the enclosing clip or `Sequence`. |
| `useDuration()` | The length of the enclosing clip or `Sequence`, in frames. |
| `<Sequence from duration>` | Shows its children from frame `from` for `duration` frames. Inside it, `useFrame()` starts at 0. Sequences nest. |
| `interpolate(x, [0, 10], [0, 1], { easing, clamp })` | Piecewise linear mapping. It holds the end values unless `clamp: false`. |
| `spring({ frame, fps, from, to, stiffness, damping, mass })` | A spring released at rest at frame 0. It is computed exactly, so any frame renders on its own. Before frame 0 it stays at `from`, so `frame: f() - 20` delays it 20 frames. |
| `<Audio src start trim volume>` | Plays a sound from frame `start` until the enclosing clip or Sequence ends. `trim={[a, b]}` plays source frames `a` to `b`. A relative `src` is found next to the project file. |
| `waitFor(promise)` | Makes the current frame wait for an image, font or data before it is captured. |
| `config` | `fps`, `width`, `height` and `frames` of the video. |
| `<SafeArea>` | For 9:16 phone video: the Instagram Reels safe zone. Put what the viewer must read inside it. |

A component must draw frame `n` the same way every time: no `Date.now()`, `Math.random()` or timers. `examples/c-quiz/src/Quiz.tsx` is a complete one: a C quiz with code, options, a countdown with ticks and an answer with a chime, laid out for 9:16.

## Studio

The Studio is the human view of a project: the same file the agent edits. It runs in a Vite dev server next to the project:

```ts
// vite.config.ts next to the project file
import { studio } from "@karkataka/studio";
import solid from "vite-plugin-solid";
export default defineConfig({ plugins: [solid(), studio({ project: "edit.json" })] });
```

```bash
cd examples/edit && ./make-media.sh                  # once: the example's media
KARKA=../../target/bundle/karka.app/Contents/MacOS/karka pnpm dev   # then open the Studio URL it prints
```

- **Preview.** It draws the project with the same player that renders it. The preview seeks the video files; a render uses exact frames cut by FFmpeg.
- **Timeline.**
  - Tracks are shown top layer first, with hide (👁) and mute (♪) per track.
  - Click or drag the ruler to seek. Click a clip to select it, and drag it to move it.
  - Play/pause (Space), loop, step one or ten frames (← →, Shift), first/last frame (Home/End).
  - Split the selected clip at the playhead (S) or delete it (⌫).
  - Undo and redo (⌘Z, ⇧⌘Z).
  - "Safe zone" (9:16 projects) outlines the Instagram Reels safe zone.
- **Inspector.**
  - With a clip selected: its fields. A component clip's props get their own fields when the component exports a `schema` (JSON Schema keywords: `type`, `title`, `description`, `enum`, `minimum`/`maximum`, plus `multiline`).
  - With nothing selected: the project's settings, and buttons to add a track or a clip at the playhead.
  - Edits are checked as you make them: a refused one shows why. Problems with the project (a missing file) are listed at the top.
- **Saved to the file.** Edits are saved to the project file as you make them. When something else changes the file (an agent running `karkataka`), the Studio shows the change at once.
- **Render.** The button renders the saved project with a progress bar, then plays `out/<project>.mp4`.

## Build and test

Do this after every code change.

**1. Run the unit tests** (a few seconds):

```bash
cargo test
pnpm test        # runtime, SDK, Studio and the c-quiz component (Vitest)
pnpm typecheck
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

**5. End-to-end tests** (about a minute). `edit.sh` works as an agent would. It builds a 16:9 edit from media using only `karkataka` commands, and checks that a mistake is refused. Then it looks at stills and renders. Its media are solid colours, so pixel checks prove frame-exact trimming, splitting, layering, boxes and the title. It also checks the size, length and sound of the MP4. `c-quiz.sh` renders the quiz project with the GPU and with `--software`, and checks that each of its 11 sounds starts within 30 ms of its time.

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

## The karka engine command

`karka` is the Rust engine. `karkataka` runs it for projects; you only need it directly to render a plain web page.

| Command | What it does |
|---|---|
| `karka render <page.html> [--frames 300] [--width 1920] [--height 1080] [--fps 30] [--props '<json>'] [--out out/render.mp4] [--progress]` | Renders a page to MP4. Sizes must be even. If the page lists `<Audio>` clips, it mixes them in as AAC. |
| `karka still <page.html> --frame <n> [the same options] [--out out/still.png]` | Renders one frame to PNG. |
| `karka smoke [--page <file>] [--frames 300] [--out out]` | The engine self-test: renders a 1080x1920 page, checks every frame and writes `frame0.png` and `smoke.mp4`. |

The page is loaded as a file, so it can use the files next to it. karka adds `<script>window.__karka = {props, frames, fps, width, height}</script>` at the end of the page. The page must define `window.__setFrame(n)`, which draws frame `n` (returning a promise if it has to wait). `@karkataka/runtime` does both.

They all take `--json`, which prints the result or error as JSON (exit code 0 on success), and `--software`. `render` and `smoke` also take:

| Option | Meaning |
|---|---|
| `--software` | Paint on the CPU instead of the GPU. Faster for most pages; the GPU is faster for 3D transforms and filters. |
| `--mode auto\|single\|multi` | `auto` (the default) checks CPU cores and free memory, and picks the number of browser processes. `single` uses one browser. `multi` uses several, one process each (`--workers N` sets how many). |
| `--encoder <name>` | The FFmpeg encoder. Default: `h264_videotoolbox` on macOS, `h264_nvenc` elsewhere. `libx264` encodes on the CPU. |

More processes only help on machines with spare cores and memory. On an 8 GB M1, one is fastest, and `auto` picks one.

## How it works

For a project, the SDK first checks the project and cuts each video clip into JPEG frames with FFmpeg (cached in `.karka/frames/`). It then builds one HTML page: the player, the project and its component modules. Then it runs karka on that page:

1. The engine loads the page in an offscreen Chromium (CEF) browser, at the project's size.
2. For each frame it calls `window.__setFrame(n)` in the page, and waits if that returns a promise.
3. It asks Chromium to paint, and takes the pixels Chromium hands back (`OnPaint`). No screenshots, no PNG per frame.
4. Chromium sometimes paints an older frame. So the engine draws the frame number into a hidden extra pixel row, and only accepts a paint whose row shows the frame it asked for.
5. It pipes the pixels into FFmpeg, which encodes on the hardware encoder.
6. Before frame 0, the page lists its audio clips: video and audio clips, and `<Audio>` in components. After the frames, a second FFmpeg run trims, delays and mixes them and adds them to the file. The video stream is copied, not encoded again.

The player draws a video clip's frame as the matching JPEG and waits until it has decoded. Every frame is therefore exact, and any format FFmpeg reads works, even ones Chromium cannot play.

A page must draw frame `n` the same way every time. To use CSS animations, pause them and seek them to the frame's time:

```js
window.__setFrame = (n) => {
  for (const a of document.getAnimations()) { a.pause(); a.currentTime = n * 1000 / 30; }
};
```

## Use the engine as a library

> Not on crates.io yet. Once published, it will be four crates: `karka` (the CLI), `karkataka-engine` (page in, raw frames out), `karkataka-encode` (frames into FFmpeg) and `karkataka-media` (audio mixing).

```rust
use karkataka_encode::{Encoder, default_h264_encoder};

fn main() -> Result<(), String> {
    // CEF and the render workers relaunch this executable. Handle that first.
    if let Some(code) = karkataka_engine::run_subprocess() {
        std::process::exit(code);
    }

    let html = r#"<div id="n" style="font-size:200px"></div>
<script>window.__setFrame = (n) => { document.getElementById("n").textContent = n; };</script>"#;
    // A page with local files (images, video frames) is loaded with karkataka_engine::file_url instead.
    let url = karkataka_engine::data_url(html);

    let mut video = Encoder::spawn("out.mp4".as_ref(), 1080, 1920, 30, default_h264_encoder())?;
    // Let the engine pick how many browser processes this machine can run.
    let workers = karkataka_engine::probe::plan(&karkataka_engine::probe::probe(), 300).workers;
    let job = karkataka_engine::Job { url: &url, width: 1080, height: 1920, frames: 300, workers, gpu: true };
    karkataka_engine::render(&job, |_frame, bgra| video.write_frame(bgra))?;
    video.finish()
}
```

Frames arrive in order, as raw BGRA pixels, on a separate thread. `render` returns the page's audio report, if it made one. `karkataka_engine::render_frame(&job, n)` renders only frame `n`.

**Building it:** set `CEF_PATH` to a CEF folder from `scripts/fetch-cef.sh`. The `cef` crate's own download is broken right now. On Linux, also add that folder to `LD_LIBRARY_PATH`. On macOS your program must run from an `.app` bundle with a helper binary; copy `crates/cli/src/bin/karka_helper.rs`, `bundle-karka.rs` and the `[package.metadata.cef.bundle]` section of `crates/cli/Cargo.toml`.

**Publishing (maintainers):** bump `version` in the root `Cargo.toml`, then with `CEF_PATH` set run `cargo publish -p` for `karkataka-encode`, `karkataka-media`, `karkataka-engine` and `karka`, in that order.

## Project layout

```
crates/
  engine/        Renders a page frame by frame
    lib.rs         Public API: Job, render, render_frame, run_subprocess, data_url, file_url
    browser.rs     One offscreen browser and its frame loop
    page.rs        The page URL and the hidden frame-number row
    worker.rs      Rendering in several processes
    probe.rs       Picks the number of processes for this machine
  encode/        Pipes frames into FFmpeg
  media/         Mixes audio clips and adds them to the video
  cli/           The karka command (main.rs: options; render.rs: render and still; smoke.rs: the smoke test)
packages/
  sdk/           @karkataka/sdk and the karkataka command
    project.ts     The project format, its fields and validation
    ops.ts         Edits: add, update, move, split, remove
    media.ts       probe, and video frames cut by FFmpeg
    render.ts      still and render: prepare media, build the player, run karka
    build.ts       Builds the player page with the project and its components
    player/        The SolidJS app that draws a project
    cli.ts         The karkataka command
  runtime/       @karkataka/runtime: useFrame, Sequence, Audio, waitFor, interpolate, spring, SafeArea
  studio/        @karkataka/studio: the Studio, a Vite plugin (plugin.ts) and its UI (app/)
examples/
  edit/          A 16:9 edit: video, logo, title, music (make-media.sh makes the media)
  c-quiz/        A 9:16 quiz short: one component clip with code, options, countdown and answer
tests/examples/  End-to-end tests: edit.sh (an agent's workflow) and c-quiz.sh
bench/           Benchmark against Remotion
tests/scenes/    Animation scenes rendered in every mode
scripts/         fetch-cef.sh
next-action-item.md   What we are doing next, and what is blocked
```

## Roadmap

- [x] Engine: render a page frame by frame and encode to MP4 (`karka smoke`)
- [x] Several browser processes, with automatic choice of how many
- [x] Projects: tracks and clips (video, image, audio, text, color, component), any size and frame rate
- [x] `karkataka` command and TypeScript SDK for agents: edit, validate, probe, still, render
- [x] SolidJS runtime for component clips: `useFrame`, `useDuration`, `Sequence`, `Audio`, `waitFor`, `interpolate`, `spring`
- [x] Audio mixed into the MP4: video sound, audio clips, `<Audio>` in components
- [x] Studio: preview, multi-track timeline with drag, split and undo, inspector, live view of agent edits, render with progress
- [ ] Keyframes: values that change over a clip (position, scale, volume) beyond fades
- [ ] Transitions between clips, speed changes, trimming by dragging clip edges in the Studio
- [ ] GPU texture path (`OnAcceleratedPaint`), so frames skip the CPU copy

**Known limits:** Studio previews have no sound. Video clips are cut into JPEG frames before a render; this takes disk space (about 0.1–0.3 MB per 1080p frame, cached in `.karka/`). NVENC (`h264_nvenc`, the Linux default) and the whole Linux build have not run yet. Each CEF upgrade can break frame capture, so the CEF version is pinned.

## License

[MIT](LICENSE). You can use, change and share it for free, including in commercial products. Videos you render with Karkataka are yours.

## Name

Karkataka is the Telugu and Kannada name for the zodiac sign Cancer, the crab. The crab points to Ferris, the Rust mascot.

---

Built by [Xploray](https://xploray.com).
