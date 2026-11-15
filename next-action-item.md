# Next action item

Rules for this file:

- Exactly one item under **Next action** at any time.
- When it is done, delete it and promote one item from **TBD**.
- When it cannot go ahead, move it to **Blocked** (with what it waits on) or **Cancelled** (with why), with the date, and promote the next item.
- Everything else we might do lives in **TBD**, most important first.

## Next action

**`bundle-karka --release`.** The bundler builds debug binaries only, so every benchmark needs release binaries copied into `target/bundle/karka.app` by hand (deleting each old file first, or macOS kills it).

- Done when: `cargo run -p karka --bin bundle-karka -- --release` builds a release `karka.app`, and `bench/run.sh "karka:--software"` runs against it with no manual copying.

## TBD

1. **Choose the default painting mode.** On the bench scene, software painting (`--software`) beat GPU painting (4.5–6.2 s vs 6.4–6.7 s) with identical pictures. Check a heavier scene (filters, big images, WebGL) before making it the default.
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

## Bench

Every speed change is compared with Remotion on the same scene.

- **Scene:** `bench/scene.html` here (300 frames, 1080x1920). The Remotion copy, with the same pixels, is in `~/Test/my-video/src/karka-bench/`, with its own entry point `index.ts` and composition `KarkaBench`. That project is real video work: leave its own scenes and `Root.tsx` alone.
- **Run both:** `bench/run.sh "karka:--software" "karka:" "remotion:--hardware-acceleration=if-possible"` (3 runs each; `RUNS=n` to change, `REMOTION_DIR` if the project moves).
- **Port:** Remotion renders on `--port=3917`, because port 3000 on this Mac is taken by an unrelated Nuxt dev server.
- **Release binaries:** until `bundle-karka --release` exists, build with `cargo build --release -p karka --bins` and copy the binaries into `target/bundle/karka.app`. Delete each old file first: copying over a binary in place makes macOS kill it on launch (exit code 137).
- **Machine:** Apple M1, 8 GB. Memory is tight: check swap (`sysctl vm.swapusage`) before trusting a slow run.
- **Latest (2026-11-15):** karka `--software` (auto mode, probe picked 1 worker) 4.4 s, karka `--software --mode multi` (2 workers) 7.3 s, karka GPU 6.4–6.7 s, Remotion hardware encoding 9.5–10.4 s, Remotion default 11.5–11.8 s.

## Blocked

- **Parallel rendering speed-up.** 2026-11-15. Waits on: a machine with more cores and memory than the 8 GB M1. `--mode auto|single|multi` is built: auto probes cores and free memory before the render and picks 1 on the M1. On a bigger machine, check that auto's choice is actually the fastest and recalibrate the constants in `crates/engine/src/probe.rs`. `--mode multi` is correct (separate processes, each with its own browser; frames still arrive in order and pass every check), but on the M1 it is slower: 2 workers 8–8.3 s and 3 workers 31–34 s, against 4.5–6.2 s for one. Even two fully independent karka processes run at 22 fps each, against 65 fps alone, so one render already uses the whole machine. (Several browsers in one process stalled each other's rendering, which is why workers are processes.)
- **Verify NVENC (`h264_nvenc`).** 2026-11-15. Waits on: a machine with an NVIDIA GPU. The code path exists and is the default on Linux and Windows, but has never run.

## Cancelled

- **Rewrite scenes or the renderer in WebAssembly.** 2026-11-15. Profiling showed scene JavaScript is near-zero cost; the time is Chromium painting, pixel copies and encoding. Dropping Chromium for a WASM 2D renderer would end "scenes are web pages", the core idea of the project.
