# Next action item

Rules for this file:

- Exactly one item under **Next action** at any time.
- When it is done, delete it and promote one item from **TBD**.
- When it cannot go ahead, move it to **Blocked** (with what it waits on) or **Cancelled** (with why), with the date, and promote the next item.
- Everything else we might do lives in **TBD**, most important first.

## Next action

**Parallel rendering in chunks.** Split the frames across several CEF browsers, render the chunks at the same time, and join them with ffmpeg. Frames depend only on their number, so chunks can run in any order.

- Done when: `karka smoke --page bench/scene.html` renders 300 frames with N workers, every frame passes the marker and cell checks, and the README benchmark table has the new time.
- Target: under 3.5 s on the M1 (now 6.4–8.0 s).

## TBD

1. **`bundle-karka --release`.** The bundler builds debug binaries only; benchmarks need release binaries copied in by hand.
2. **`karka still`.** Render one frame of a page and save it as PNG. Almost free with the current engine.
3. **`@karkataka/runtime` and `karka render`.** `useFrame`, `interpolate`, `Sequence`, `delayRender`, so real scenes can render. The scene root must be sized in px (not `vh`), because the engine's hidden marker row makes the view 1 px taller.
4. **Profile the per-frame cost.** Find how much time goes to the ~3 copies per frame and ffmpeg's CPU BGRA-to-YUV conversion before optimising them.
5. **Fix the cef crate's CEF download upstream.** Its index download fails because the CDN's index.json is over the 10 MB limit. A fix in tauri-apps/cef-rs would let us drop `scripts/fetch-cef.sh` and the `CEF_VERSION` install step.
6. **CI on Linux and macOS** running `cargo test` and `karka smoke`. The Linux path has never run.
7. **CLI options** for fps, size and stall timeout (now fixed at 30 fps, 1080x1920, 10 s).
8. **JSON progress lines** (`--json`) during long renders, so AI agents can follow progress.
9. **`OnAcceleratedPaint` GPU path.** Frames go from Chromium to the encoder without a CPU copy. Only worth it after item 4.
10. **Audio muxing.**
11. **Studio.** Scrubber, props panel, render button.
12. **Publish to crates.io.** Bump to 0.1.0 and publish in the order in the README.

## Blocked

- **Verify NVENC (`h264_nvenc`).** 2026-11-15. Waits on: a machine with an NVIDIA GPU. The code path exists and is the default on Linux and Windows, but has never run.

## Cancelled

- **Rewrite scenes or the renderer in WebAssembly.** 2026-11-15. Profiling showed scene JavaScript is near-zero cost; the time is Chromium painting, pixel copies and encoding. Dropping Chromium for a WASM 2D renderer would end "scenes are web pages", the core idea of the project.
