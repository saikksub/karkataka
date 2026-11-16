# Next action item

Rules for this file:

- Exactly one item under **Next action** at any time.
- When it is done, delete it and promote one item from **TBD**.
- When it cannot go ahead, move it to **Blocked** (with what it waits on) or **Cancelled** (with why), with the date, and promote the next item.
- Everything else we might do lives in **TBD**, most important first.

## Next action

**`bundle-karka --release`.** The bundler builds debug binaries only, so every benchmark and every real use needs release binaries copied into `target/bundle/karka.app` by hand (deleting each old file first, or macOS kills it). Since 2026-11-16 karka is one binary that people use directly, so this is also how they install it.

- Done when: `cargo run -p karka --bin bundle-karka -- --release` builds a release `karka.app`, and `bench/run.sh "karka:--software"` runs against it with no manual copying.

## TBD

1. **Choose the default painting mode.** Neither wins everywhere. In the scene tests (`tests/scenes/run.sh`, 2026-11-15), GPU painting was faster on `css-3d` (72 vs 36 fps) and `filters` (64 vs 47 fps), and CPU painting (`--software`) was faster on `dom-stress` (39 vs 22 fps) and the bench scene. Other scenes were close. Since 2026-11-15 `--software` disables only GPU compositing, so WebGL still works. Option: let auto mode pick per scene with a short trial render.
2. **Keyframes for clip fields** (position, scale, rotation, opacity, volume over time), so agents can animate without writing a component. Fades are the only change over a clip today (2026-11-16). This also gives audio fades and ducking.
3. **Transitions and speed.** Cross-fades and wipes between clips on one track, and video speed (slow motion, fast forward). An agent can only cut or fade today.
4. **Studio editing by hand:** trim by dragging clip edges, drag clips between tracks, zoom the timeline, and add media by dropping files. Today you move clips only within their track, and other changes go through the inspector.
5. **Video frames without the JPEG cache.** Every video clip is cut into JPEGs before a render (0.1–0.3 MB per 1080p frame, in `.karka/frames/`, never cleaned). Stream decoded frames to the page instead, or at least clean old cache folders.
6. **Profile the per-frame cost.** Find how much time goes to the ~3 copies per frame and ffmpeg's CPU BGRA-to-YUV conversion before optimising them.
7. **Fix the cef crate's CEF download upstream.** Its index download fails because the CDN's index.json is over the 10 MB limit. A fix in tauri-apps/cef-rs would let us drop `scripts/fetch-cef.sh` and the `CEF_VERSION` install step.
8. **CI on Linux and macOS** running `cargo test` and `tests/scenes/run.sh`. The Linux path has never run.
9. **JSON progress lines on stdout with `--json`.** `karka render --progress` prints them to stderr since 2026-11-15 (the Studio uses it); `smoke` has none.
10. **`OnAcceleratedPaint` GPU path.** Frames go from Chromium to the encoder without a CPU copy. Only worth it after item 6 (profiling).
11. **npm packages in scenes.** Scenes compile inside karka's Chromium with no bundler, so they can import only solid-js, the runtime and project files (2026-11-16). Option: let a project vendor ES modules into a folder that the import map serves.
12. **Studio: audio in the preview, and cancelling a render.** The preview is silent, and a render stops only when the Studio tab closes (2026-11-15).
13. **Install without building.** `karka` 0.0.0 is on crates.io since 2026-11-16, but `cargo install karka` cannot run on macOS (CEF needs an .app, and `bundle-karka` runs `cargo build` in the source checkout), and users still need `scripts/fetch-cef.sh` and `CEF_PATH`, which the crate does not ship. Ship a prebuilt release `karka.app` (zip on GitHub Releases, then a Homebrew tap) once `bundle-karka --release` exists (the next action).

## Bench

Every speed change is compared with Remotion on the same scene.

- **Scene:** `bench/scene.html` here (300 frames, 1080x1920). The Remotion copy, with the same pixels, is in `~/Test/my-video/src/karka-bench/`, with its own entry point `index.ts` and composition `KarkaBench`. That project is real video work: leave its own scenes and `Root.tsx` alone.
- **Run both:** `bench/run.sh "karka:--software" "karka:" "remotion:--hardware-acceleration=if-possible"` (3 runs each; `RUNS=n` to change, `REMOTION_DIR` if the project moves).
- **Port:** Remotion renders on `--port=3917`, because port 3000 on this Mac is taken by an unrelated Nuxt dev server.
- **Release binaries:** until `bundle-karka --release` exists, build with `cargo build --release -p karka --bins` and copy the binaries into `target/bundle/karka.app`. Delete each old file first: copying over a binary in place makes macOS kill it on launch (exit code 137).
- **Machine:** Apple M1, 8 GB. Memory is tight: check swap (`sysctl vm.swapusage`) before trusting a slow run.
- **Latest (2026-11-15, after the redraw fix):** karka `--software` 4.2–4.3 s, karka `--software --mode multi` (2 workers) 6.1–6.8 s, karka GPU 6.5 s, Remotion hardware encoding 9.6–10.3 s, Remotion default 11.5–11.8 s.
- **Redraw count:** `KARKA_DEBUG=1` prints begin frames sent. Healthy is a few per frame (bench scene: GPU 306, software 1973 for 300 frames). Thousands per frame means redraws are multiplying again.

## Blocked

- **Parallel rendering speed-up.** 2026-11-15. Waits on: a machine with more cores and memory than the 8 GB M1. `--mode auto|single|multi` is built: auto probes cores and free memory before the render and picks 1 on the M1. On a bigger machine, check that auto's choice is actually the fastest and recalibrate the constants in `crates/engine/src/probe.rs`. `--mode multi` is correct (separate processes, each with its own browser; frames still arrive in order and pass every check), but on the M1 it is slower: 2 workers 8–8.3 s and 3 workers 31–34 s, against 4.5–6.2 s for one. Even two fully independent karka processes run at 22 fps each, against 65 fps alone, so one render already uses the whole machine. (Several browsers in one process stalled each other's rendering, which is why workers are processes.)
- **Verify NVENC (`h264_nvenc`).** 2026-11-15. Waits on: a machine with an NVIDIA GPU. The code path exists and is the default on Linux and Windows, but has never run.

## Cancelled

- **Rewrite scenes or the renderer in WebAssembly.** 2026-11-15. Profiling showed scene JavaScript is near-zero cost; the time is Chromium painting, pixel copies and encoding. Dropping Chromium for a WASM 2D renderer would end "scenes are web pages", the core idea of the project.
