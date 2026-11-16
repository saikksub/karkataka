import type { PropSchema, PropsSchema } from "@karkataka/runtime";
import { createEffect, createSignal, For, Index, Match, on, onCleanup, onMount, Show, Switch, untrack } from "solid-js";
import { kindOf, propNames, toNumber } from "../fields.ts";
import type { RenderEvent } from "../plugin.ts";
import { clampFrame, playhead } from "../timeline.ts";

const FPS = 30;
const WIDTH = 1080;
const HEIGHT = 1920;
type Props = Record<string, unknown>;

// Edited props, frame count and position survive reloads (a scene edit Vite cannot hot-swap reloads this page too).
const STORE = "karka-studio";
const saved: { edits?: Props; frames?: number; frame?: number } = (() => {
  try {
    return JSON.parse(localStorage.getItem(STORE) ?? "{}");
  } catch {
    return {};
  }
})();

/** The scene page with props and frames passed the way the runtime reads them outside karka. */
const sceneUrl = (props: Props, frames: number) => `/?karka=${encodeURIComponent(JSON.stringify({ props, frames }))}`;

export function App() {
  const [frames, setFrames] = createSignal(saved.frames ?? 300);
  const [frame, setFrame] = createSignal(clampFrame(saved.frame ?? 0, frames()));
  const [edits, setEdits] = createSignal<Props>(saved.edits ?? {});
  const [scene, setScene] = createSignal<{ defaults: object; schema?: PropsSchema }>();
  const [sceneWindow, setSceneWindow] = createSignal<Window>();
  const [playing, setPlaying] = createSignal(false);
  const [loop, setLoop] = createSignal(true);
  const [safeZone, setSafeZone] = createSignal(true);
  const [src, setSrc] = createSignal(sceneUrl(edits(), frames()));
  const props = (): Props => ({ ...scene()?.defaults, ...edits() });

  // The scene reads its props once when it loads, so a change reloads it (after typing pauses).
  let reload: ReturnType<typeof setTimeout>;
  createEffect(on([edits, frames], () => {
    clearTimeout(reload);
    reload = setTimeout(() => setSrc(sceneUrl(edits(), frames())), 300);
  }, { defer: true }));
  createEffect(() => localStorage.setItem(STORE, JSON.stringify({ edits: edits(), frames: frames(), frame: frame() })));
  createEffect(() => setFrame((f) => clampFrame(f, frames())));
  // Draw the current frame in the scene, the same call karka makes.
  createEffect(() => sceneWindow()?.__setFrame?.(frame()));

  let raf = 0;
  // Runs on play/pause only: each frame it sets must not restart the clock.
  createEffect(on(playing, (play) => {
    cancelAnimationFrame(raf);
    if (!play) return;
    const from = untrack(() => (frame() >= frames() - 1 ? 0 : frame()));
    const started = performance.now();
    const tick = (now: number) => {
      const at = playhead(from, now - started, FPS, frames(), loop());
      setFrame(at.frame);
      if (at.ended) setPlaying(false);
      else raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
  }));
  onCleanup(() => cancelAnimationFrame(raf));
  const step = (by: number) => {
    setPlaying(false);
    setFrame((f) => clampFrame(f + by, frames()));
  };

  onMount(() => {
    const keys = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return;
      const actions: Record<string, () => void> = {
        " ": () => setPlaying((p) => !p),
        ArrowLeft: () => step(e.shiftKey ? -10 : -1),
        ArrowRight: () => step(e.shiftKey ? 10 : 1),
        Home: () => step(-frames()),
        End: () => step(frames()),
      };
      if (actions[e.key]) {
        e.preventDefault();
        actions[e.key]();
      }
    };
    window.addEventListener("keydown", keys);
    onCleanup(() => window.removeEventListener("keydown", keys));
  });

  const onSceneLoad = (iframe: HTMLIFrameElement) => {
    const w = iframe.contentWindow!;
    setScene(w.__karkaScene);
    setSceneWindow(() => w);
    w.__setFrame?.(frame());
  };

  return (
    <div class="studio">
      <main>
        <Preview src={src()} safeZone={safeZone()} onLoad={onSceneLoad} />
        <div class="timeline">
          <input
            type="range"
            class="scrubber"
            aria-label="Frame"
            min="0"
            max={frames() - 1}
            value={frame()}
            onInput={(e) => {
              setPlaying(false);
              setFrame(Number(e.currentTarget.value));
            }}
          />
          <div class="controls">
            <button title="First frame (Home)" onClick={() => step(-frames())}>⏮</button>
            <button title="Previous frame (←, Shift+← for 10)" onClick={() => step(-1)}>◀</button>
            <button class="play" title="Play / pause (Space)" onClick={() => setPlaying((p) => !p)}>{playing() ? "❚❚" : "▶"}</button>
            <button title="Next frame (→, Shift+→ for 10)" onClick={() => step(1)}>▶</button>
            <button title="Last frame (End)" onClick={() => step(frames())}>⏭</button>
            <label><input type="checkbox" checked={loop()} onChange={(e) => setLoop(e.currentTarget.checked)} /> Loop</label>
            <label><input type="checkbox" checked={safeZone()} onChange={(e) => setSafeZone(e.currentTarget.checked)} /> Safe zone</label>
            <span class="time">
              frame {frame()} / {frames() - 1} · {(frame() / FPS).toFixed(2)} s
            </span>
            <label class="frames">
              Frames{" "}
              <input
                type="number"
                min="1"
                value={frames()}
                onChange={(e) => setFrames(Math.max(1, Math.round(Number(e.currentTarget.value)) || 1))}
              />
            </label>
          </div>
        </div>
      </main>
      <aside>
        <section>
          <header>
            <h2>Props</h2>
            <button class="link" disabled={Object.keys(edits()).length === 0} onClick={() => setEdits({})}>Reset</button>
          </header>
          <Show when={scene()} fallback={<p class="muted">The scene has not called getProps().</p>}>
            {(s) => (
              <For each={propNames(s().defaults, s().schema)}>
                {(name) => (
                  <Field
                    name={name}
                    schema={s().schema?.[name]}
                    value={props()[name]}
                    onChange={(v) => setEdits((e) => ({ ...e, [name]: v }))}
                  />
                )}
              </For>
            )}
          </Show>
        </section>
        <RenderPanel props={props()} frames={frames()} />
      </aside>
    </div>
  );
}

function Preview(p: { src: string; safeZone: boolean; onLoad: (iframe: HTMLIFrameElement) => void }) {
  let box!: HTMLDivElement;
  const [scale, setScale] = createSignal(0.3);
  onMount(() => {
    const fit = new ResizeObserver(([e]) => setScale(Math.min(e.contentRect.width / WIDTH, e.contentRect.height / HEIGHT)));
    fit.observe(box);
    onCleanup(() => fit.disconnect());
  });
  return (
    <div class="preview" ref={box}>
      <div class="frame" style={{ width: `${WIDTH * scale()}px`, height: `${HEIGHT * scale()}px` }}>
        <iframe
          title="Scene"
          src={p.src}
          width={WIDTH}
          height={HEIGHT}
          style={{ transform: `scale(${scale()})` }}
          onLoad={(e) => p.onLoad(e.currentTarget)}
        />
        {/* Instagram Reels safe zone of a 1080x1920 frame: top 250, right 200, bottom 440, left 96. */}
        <Show when={p.safeZone}>
          <div class="safe" style={{ top: `${250 * scale()}px`, right: `${200 * scale()}px`, bottom: `${440 * scale()}px`, left: `${96 * scale()}px` }} />
        </Show>
      </div>
    </div>
  );
}

function Field(p: { name: string; schema?: PropSchema; value: unknown; onChange: (value: unknown) => void }) {
  const kind = () => kindOf(p.schema, p.value);
  const id = `prop-${p.name}`;
  const [jsonError, setJsonError] = createSignal(false);
  return (
    <div class="field">
      <label for={id}>{p.schema?.title ?? p.name}</label>
      <Switch>
        <Match when={kind() === "text"}>
          <input id={id} value={String(p.value ?? "")} onInput={(e) => p.onChange(e.currentTarget.value)} />
        </Match>
        <Match when={kind() === "multiline"}>
          <textarea id={id} rows={Math.min(12, String(p.value ?? "").split("\n").length + 1)} value={String(p.value ?? "")} onInput={(e) => p.onChange(e.currentTarget.value)} />
        </Match>
        <Match when={kind() === "number"}>
          <input
            id={id}
            type="number"
            min={p.schema?.minimum}
            max={p.schema?.maximum}
            step={p.schema?.type === "integer" ? 1 : "any"}
            value={String(p.value ?? "")}
            onChange={(e) => {
              const n = toNumber(p.schema, e.currentTarget.value);
              if (n !== undefined) p.onChange(n);
              e.currentTarget.value = String(n ?? p.value ?? "");
            }}
          />
        </Match>
        <Match when={kind() === "boolean"}>
          <input id={id} type="checkbox" checked={Boolean(p.value)} onChange={(e) => p.onChange(e.currentTarget.checked)} />
        </Match>
        <Match when={kind() === "select"}>
          <select id={id} onChange={(e) => p.onChange(p.schema!.enum![e.currentTarget.selectedIndex])}>
            <For each={p.schema!.enum}>{(option) => <option selected={option === p.value}>{String(option)}</option>}</For>
          </select>
        </Match>
        <Match when={kind() === "list"}>
          <div class="list" id={id}>
            {/* Index, not For: For keys items by value, so editing one would replace its input and lose focus. */}
            <Index each={(p.value as string[] | undefined) ?? []}>
              {(item, i) => (
                <div class="item">
                  <input
                    aria-label={`${p.name} ${i + 1}`}
                    value={item()}
                    onInput={(e) => p.onChange((p.value as string[]).map((v, j) => (j === i ? e.currentTarget.value : v)))}
                  />
                  <button title="Remove" onClick={() => p.onChange((p.value as string[]).filter((_, j) => j !== i))}>✕</button>
                </div>
              )}
            </Index>
            <button class="link" onClick={() => p.onChange([...((p.value as string[] | undefined) ?? []), ""])}>+ Add</button>
          </div>
        </Match>
        <Match when={kind() === "json"}>
          <textarea
            id={id}
            class={jsonError() ? "invalid" : ""}
            rows={4}
            value={JSON.stringify(p.value, null, 2)}
            onChange={(e) => {
              try {
                p.onChange(JSON.parse(e.currentTarget.value));
                setJsonError(false);
              } catch {
                setJsonError(true);
              }
            }}
          />
        </Match>
      </Switch>
      <Show when={p.schema?.description}>
        <p class="muted">{p.schema!.description}</p>
      </Show>
    </div>
  );
}

type Job =
  | { state: "idle" }
  | { state: "building" }
  | { state: "rendering"; frame: number; frames: number }
  | { state: "done"; url: string; secs: number }
  | { state: "error"; message: string };

function RenderPanel(p: { props: Props; frames: number }) {
  const [job, setJob] = createSignal<Job>({ state: "idle" });
  const busy = () => job().state === "building" || job().state === "rendering";

  async function start() {
    const started = performance.now();
    setJob({ state: "building" });
    const handle = (event: RenderEvent) => {
      if (event.type === "progress") setJob({ state: "rendering", frame: event.frame, frames: event.frames });
      else if (event.type === "done") setJob({ state: "done", url: event.url, secs: (performance.now() - started) / 1000 });
      else if (event.type === "error") setJob({ state: "error", message: event.message });
    };
    try {
      const res = await fetch("/__karka/render", { method: "POST", body: JSON.stringify({ props: p.props, frames: p.frames }) });
      const reader = res.body!.pipeThrough(new TextDecoderStream()).getReader();
      let buffer = "";
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += value;
        const lines = buffer.split("\n");
        buffer = lines.pop()!;
        lines.forEach((line) => handle(JSON.parse(line)));
      }
      if (busy()) setJob({ state: "error", message: "the dev server stopped before the render finished" });
    } catch (e) {
      setJob({ state: "error", message: e instanceof Error ? e.message : String(e) });
    }
  }

  return (
    <section>
      <header>
        <h2>Render</h2>
      </header>
      <button class="render" disabled={busy()} onClick={start}>
        {busy() ? "Rendering…" : `Render ${p.frames} frames to MP4`}
      </button>
      <Switch>
        <Match when={job().state === "building"}>
          <progress aria-label="Render progress" />
          <p class="muted">Building the scene…</p>
        </Match>
        <Match when={job().state === "rendering" && (job() as Job & { state: "rendering" })}>
          {(j) => (
            <>
              <progress aria-label="Render progress" value={j().frame} max={j().frames} />
              <p class="muted">
                Frame {j().frame} of {j().frames} · {Math.round((100 * j().frame) / j().frames)}%
              </p>
            </>
          )}
        </Match>
        <Match when={job().state === "done" && (job() as Job & { state: "done" })}>
          {(j) => (
            <>
              <p class="ok">
                Done in {j().secs.toFixed(1)} s · <a href={j().url} target="_blank">out/studio.mp4</a>
              </p>
              <video src={j().url} controls />
            </>
          )}
        </Match>
        <Match when={job().state === "error" && (job() as Job & { state: "error" })}>
          {(j) => <pre class="error">{j().message}</pre>}
        </Match>
      </Switch>
    </section>
  );
}
