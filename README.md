# Karkataka

Write video scenes as web pages. Render them on the GPU.

Karkataka is a programmatic video renderer built by [Xploray](https://xploray.com). You build each scene with TypeScript, HTML and CSS, preview and review it in a browser, and render it to MP4 through a Rust engine that drives Chromium (CEF) offscreen and encodes on the GPU.

> Status: early development. APIs will change between releases.

## Why

Remotion takes a screenshot of a headless browser for every frame, saves it as an image, and hands it to FFmpeg. That screenshot step is the bottleneck.

Karkataka skips it:

- Chromium draws each frame on the GPU and hands raw pixels (or a GPU texture) straight to the engine.
- No screenshots and no PNG encoding per frame.
- Encoding uses the hardware encoder (NVENC, VAAPI/QSV, VideoToolbox), with an FFmpeg CPU fallback.

## How it works

```
Studio (web UI)  ──►  Scene page (TypeScript)  ──►  Engine (Rust)
 timeline, props,       draws frame N                CEF offscreen
 review, queue                                        → HW encoder → MP4
```

1. The engine loads the scene page in an offscreen CEF browser.
2. For each frame, it calls `window.__setFrame(n)` and waits until the page is ready.
3. It tells Chromium to paint one frame (external begin frame).
4. It takes the frame from `OnPaint` (CPU buffer) or `OnAcceleratedPaint` (GPU texture).
5. It sends the frame to the encoder, then muxes the audio at the end.

Every frame is a pure function of `frame + props`. The same input always gives the same picture, so frames can render in any order and split across workers.

## Writing a scene

```ts
import { useFrame, interpolate, Sequence } from "@karkataka/runtime";

export default function Quiz({ question, seconds }) {
  const frame = useFrame();
  const opacity = interpolate(frame, [0, 15], [0, 1]);

  return (
    <div style={{ opacity }}>
      <Sequence from={0} duration={seconds * 30}>
        <Code source={question} />
        <Timer seconds={seconds} />
      </Sequence>
    </div>
  );
}
```

Rules for scenes:

- Use `useFrame()` for all timing. CSS animations, `setTimeout` and `requestAnimationFrame` are blocked, because they break repeatable output.
- Wrap anything that loads (fonts, images, data) in `delayRender()` so the frame is not captured early.
- Use `<Video>` and `<Audio>` for media. Video frames are decoded in Rust, not by seeking an HTML `<video>` element.

## Studio

- Timeline scrubber, play, loop, and frame stepping
- Props panel generated from the scene's schema
- Hot reload on save
- Review mode: comments pinned to a frame number, then approve and render
- Render queue with progress per job

## CLI

The CLI is the main interface. Everything Karkataka can do is a `karka` command, so scripts, CI and AI agents can drive it without the Studio. The Studio is a UI on top of the same commands.

- Every command runs without prompts.
- `--help` on any command lists its options.
- `--json` prints results and errors as JSON on stdout.
- The exit code is 0 on success and non-zero on failure.

```bash
karka studio                       # open the Studio
karka render quiz-12               # render with defaults
karka render quiz-12 --codec h265 --crf 20 --fps 60
karka still quiz-12 --frame 90     # export one frame as PNG
```

## Render options

| Group | Options |
|---|---|
| Output | MP4, WebM, MOV, GIF, PNG sequence, single still |
| Codec | H.264, H.265, AV1, VP9, ProRes 422 / 4444 |
| Quality | CRF, bitrate, encoder preset, keyframe interval |
| Frame | Resolution, scale factor, fps, frame range |
| Color | Pixel format (yuv420p, yuv444p), alpha channel, color space |
| Speed | GPU on/off, worker count, chunk size |

## Project layout

```
crates/
  engine/      CEF control, frame stepping, capture
  encode/      Hardware encoders and FFmpeg fallback
  media/       Video decoding and audio mixing
  cli/         The `karka` command
packages/
  runtime/     useFrame, interpolate, spring, Sequence, delayRender
  studio/      Web UI for preview, review and render queue
examples/
  c-quiz/      Vertical C programming quiz short
```

## Requirements

- Rust (stable)
- Node.js and pnpm (for the Studio and runtime)
- CEF binaries (version pinned in `cef.lock`, downloaded by `scripts/fetch-cef.sh`)
- FFmpeg, installed separately (see below)
- Optional: NVIDIA, Intel or AMD GPU with hardware encoding support

## Setup

### 1. Install FFmpeg separately

Karkataka does not ship FFmpeg. Every frame is piped into the `ffmpeg` command, so it must be installed and on your `PATH`.

```bash
brew install ffmpeg        # macOS
sudo apt install ffmpeg    # Debian / Ubuntu
```

Check that your build has the hardware encoder you need:

```bash
ffmpeg -hide_banner -encoders | grep -E "h264_(nvenc|videotoolbox)"
```

- `h264_nvenc` needs an NVIDIA GPU and an FFmpeg built with NVENC. It is the default on Linux and Windows.
- `h264_videotoolbox` is the default on macOS, which has no NVENC.
- Pass `--encoder <name>` to use any other FFmpeg encoder, e.g. `--encoder libx264` for CPU encoding.

### 2. Get the code and CEF

```bash
git clone https://github.com/xploray/karkataka
cd karkataka
./scripts/fetch-cef.sh     # downloads the CEF build pinned in cef.lock into .cef/ (about 135 MB)
pnpm install               # JS dependencies
```

`.cargo/config.toml` points the `cef` crate at `.cef/`, so no environment variables are needed.

### 3. Build and run the engine smoke test

The smoke test renders a built-in 1080x1920 page offscreen for 300 frames. It saves frame 0 as `out/frame0.png`, checks that every frame is unique and in order, and encodes `out/smoke.mp4`.

**macOS.** CEF must run from an `.app` bundle, so build the bundle first:

```bash
cargo run -p karka --bin bundle-karka
target/bundle/karka.app/Contents/MacOS/karka smoke --out out
```

**Linux.**

```bash
cargo run -p karka -- smoke --out out
```

Add `--json` for machine-readable output.

### Day-to-day commands

```bash
cargo build           # build all Rust crates
cargo test            # run Rust tests
pnpm typecheck        # typecheck all TypeScript packages
```

The repo is two workspaces side by side: a Cargo workspace (`crates/*`) and a pnpm workspace (`packages/*`, `examples/*`).

## Roadmap

- [x] Engine: load a page in CEF, step 300 frames, encode with NVENC (`karka smoke`; verified on macOS with VideoToolbox, NVENC not yet run on NVIDIA hardware)
- [ ] Runtime: `useFrame`, `interpolate`, `Sequence`, `delayRender`
- [ ] FFmpeg fallback and audio muxing
- [ ] Studio: scrubber, props panel, render button
- [ ] Parallel rendering in chunks
- [ ] `<Video>` input with hardware decoding
- [ ] Review mode with frame comments
- [ ] `OnAcceleratedPaint` GPU texture path on Linux

## Known limits

- The GPU texture path (`OnAcceleratedPaint`) is least tested on Linux. `OnPaint` is the default until it is verified.
- Each CEF upgrade can break capture. The CEF version is pinned.
- Fonts and images must load through `delayRender()`, or frames can flicker between chunks.

## Contributing

Issues and pull requests are welcome. For larger changes, open an issue first so the approach can be agreed before you write code.

## License

Karkataka is licensed under the [MIT License](LICENSE).

You can use, change and share it for free, including in commercial and closed-source products. Keep the copyright notice and license text in your copies. Videos you render with Karkataka are yours.

## Name

Karkataka is the Telugu and Kannada name for the zodiac sign Cancer, the crab. The crab points to Ferris, the Rust mascot.

---

Built by [Xploray](https://xploray.com).
