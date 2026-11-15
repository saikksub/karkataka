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
- CEF binaries (version pinned in `cef.lock`)
- FFmpeg (for the fallback encoder and muxing)
- Optional: NVIDIA, Intel or AMD GPU with hardware encoding support

## Roadmap

- [ ] Engine: load a page in CEF, step 300 frames, encode with NVENC
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
