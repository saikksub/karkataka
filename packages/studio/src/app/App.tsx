import type { Clip, Project } from "@karkataka/player/project";
import type { PropSchema } from "@karkataka/runtime";
import { createEffect, createMemo, createSignal, For, Match, on, onCleanup, onMount, Show, Switch, untrack } from "solid-js";
import { api, durationOf, findClip, forPlayer, scenesOf, type Edit, type Fields, type Problem, type RenderEvent } from "../api.ts";
import { propNames } from "../fields.ts";
import { clampFrame, playhead } from "../timeline.ts";
import { Field } from "./Field.tsx";
import { Timeline } from "./Timeline.tsx";

const CLIP_TYPES = ["video", "image", "audio", "text", "color", "component"] as const;
type ClipType = (typeof CLIP_TYPES)[number];

export function App() {
  const [state, setState] = createSignal<{ project: Project; file: string }>();
  const [fields, setFields] = createSignal<{ project: Fields; clips: Record<string, Fields> }>({ project: {}, clips: {} });
  const [scenesChanged, setScenesChanged] = createSignal(0);
  const [problems, setProblems] = createSignal<Problem[]>([]);
  const [error, setError] = createSignal("");
  const [selected, setSelected] = createSignal<string>();
  const [frame, setFrame] = createSignal(0);
  const [playing, setPlaying] = createSignal(false);
  const [loop, setLoop] = createSignal(true);
  const [safeZone, setSafeZone] = createSignal(true);
  // Not compared: a reloaded page keeps the same Window object, and must still get the project.
  const [player, setPlayer] = createSignal<Window | undefined>(undefined, { equals: false });
  const project = () => state()?.project;
  const duration = () => (project() ? durationOf(project()!) : 1);
  const fps = () => project()?.fps ?? 30;

  // Undo history. Edits to the same field in quick succession (typing) count as one step.
  const past: Project[] = [];
  let future: Project[] = [];
  let lastKey = "";
  let lastAt = 0;
  let saved = "";

  function show(s: { project: Project; file: string; problems: Problem[] }) {
    saved = JSON.stringify(s.project);
    setProblems(s.problems);
    setState({ project: s.project, file: s.file });
  }

  async function load() {
    try {
      const s = await api.project();
      // Our own edit coming back: nothing new.
      if (JSON.stringify(s.project) === saved) return setProblems(s.problems);
      if (project()) past.push(project()!);
      show(s);
    } catch (e) {
      setError((e as Error).message);
    }
  }

  // Edits run one after another, and only the last one's result is shown: typing sends an edit
  // per key, and an older answer must not overwrite what was typed since.
  let queue: Promise<unknown> = Promise.resolve();
  let waiting = 0;
  /** Make an edit through karka (the same rules as the CLI); show why when it is refused. */
  function edit(change: Edit, key = "") {
    waiting++;
    const run = async () => {
      const before = project()!;
      try {
        const s = await api.edit(change);
        if (key === "" || key !== lastKey || Date.now() - lastAt > 1000) past.push(before);
        [lastKey, lastAt, future] = [key, Date.now(), []];
        setError("");
        if (waiting === 1) show(s);
        else saved = JSON.stringify(s.project);
        return s.result;
      } catch (e) {
        setError((e as Error).message);
        // Show the project as it is, so a field that was refused shows its real value again.
        setState((s) => ({ ...s!, project: structuredClone(s!.project) }));
      } finally {
        waiting--;
      }
    };
    const result = queue.then(run);
    queue = result;
    return result;
  }
  async function replace(next: Project) {
    try {
      show(await api.replace(next));
    } catch (e) {
      setError((e as Error).message);
    }
  }
  const undo = () => past.length && (future.push(project()!), replace(past.pop()!));
  const redo = () => future.length && (past.push(project()!), replace(future.pop()!));

  onMount(() => {
    load();
    api.fields().then(setFields, (e) => setError(e.message));
    // A file changed on disk: the project (an agent's edit) or a scene (yours, in your editor).
    const events = new EventSource("/api/events");
    events.addEventListener("project", load);
    events.addEventListener("scenes", () => setScenesChanged((n) => n + 1));
    onCleanup(() => events.close());
  });

  // The player page compiles the scenes once when it loads: a new size, fps, scene file or edited
  // scene loads it again.
  const playerUrl = createMemo(() => {
    const p = project();
    if (!p) return undefined;
    const config = { fps: p.fps, width: p.width, height: p.height, frames: durationOf(p), props: {} };
    return `/player?karka=${encodeURIComponent(JSON.stringify(config))}&scenes=${encodeURIComponent(scenesOf(p))}&v=${scenesChanged()}`;
  });
  createEffect(() => {
    const p = project();
    const w = player();
    if (p && w?.__karkaSetProject) {
      w.__karkaSetProject(forPlayer(p));
      w.__setFrame?.(untrack(frame));
    }
  });
  createEffect(() => player()?.__setFrame?.(frame()));
  createEffect(() => setFrame((f) => clampFrame(f, duration())));

  let raf = 0;
  createEffect(on(playing, (play) => {
    cancelAnimationFrame(raf);
    if (!play) return;
    const from = untrack(() => (frame() >= duration() - 1 ? 0 : frame()));
    const started = performance.now();
    const tick = (now: number) => {
      const at = playhead(from, now - started, fps(), duration(), loop());
      setFrame(at.frame);
      if (at.ended) setPlaying(false);
      else raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
  }));
  onCleanup(() => cancelAnimationFrame(raf));
  const step = (by: number) => {
    setPlaying(false);
    setFrame((f) => clampFrame(f + by, duration()));
  };

  onMount(() => {
    const keys = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
        if (typing) return;
        e.preventDefault();
        return e.shiftKey ? redo() : undo();
      }
      if (typing) return;
      const actions: Record<string, () => void> = {
        " ": () => setPlaying((p) => !p),
        ArrowLeft: () => step(e.shiftKey ? -10 : -1),
        ArrowRight: () => step(e.shiftKey ? 10 : 1),
        Home: () => step(-duration()),
        End: () => step(duration()),
        Delete: () => selected() && (edit({ op: "remove", id: selected()! }), setSelected(undefined)),
        Backspace: () => selected() && (edit({ op: "remove", id: selected()! }), setSelected(undefined)),
        s: () => selected() && edit({ op: "split", clip: selected()!, at: frame() }),
      };
      if (actions[e.key]) {
        e.preventDefault();
        actions[e.key]();
      }
    };
    window.addEventListener("keydown", keys);
    onCleanup(() => window.removeEventListener("keydown", keys));
  });

  const clip = () => (project() ? findClip(project()!, selected()) : undefined);
  const portrait = () => project() && project()!.width * 16 === project()!.height * 9;

  return (
    <Show when={project()} fallback={<p class="muted loading">{error() || "Loading the project…"}</p>}>
      <div class="studio">
        <main>
          <div class="top">
            <Preview
              url={playerUrl()!}
              width={project()!.width}
              height={project()!.height}
              safeZone={Boolean(portrait()) && safeZone()}
              // The page compiles its scenes after it loads; send it the project and frame once it can draw.
              onLoad={(w) => void Promise.resolve(w.__karkaReady).then(() => setPlayer(() => w), () => setPlayer(() => w))}
            />
          </div>
          <div class="timeline">
            <div class="controls">
              <button title="First frame (Home)" onClick={() => step(-duration())}>⏮</button>
              <button title="Previous frame (←, Shift+← for 10)" onClick={() => step(-1)}>◀</button>
              <button class="play" title="Play / pause (Space)" onClick={() => setPlaying((p) => !p)}>{playing() ? "❚❚" : "▶"}</button>
              <button title="Next frame (→, Shift+→ for 10)" onClick={() => step(1)}>▶</button>
              <button title="Last frame (End)" onClick={() => step(duration())}>⏭</button>
              <label><input type="checkbox" checked={loop()} onChange={(e) => setLoop(e.currentTarget.checked)} /> Loop</label>
              <Show when={portrait()}>
                <label title="Instagram Reels safe zone (9:16)"><input type="checkbox" checked={safeZone()} onChange={(e) => setSafeZone(e.currentTarget.checked)} /> Safe zone</label>
              </Show>
              <button title="Undo (⌘Z)" onClick={undo}>↶</button>
              <button title="Redo (⇧⌘Z)" onClick={redo}>↷</button>
              <span class="time">
                frame {frame()} / {duration() - 1} · {(frame() / fps()).toFixed(2)} s · {project()!.width}×{project()!.height} @ {fps()}
              </span>
            </div>
            <Timeline
              project={project()!}
              duration={duration()}
              fps={fps()}
              frame={frame()}
              selected={selected()}
              onSeek={(f) => (setPlaying(false), setFrame(clampFrame(f, duration())))}
              onSelect={setSelected}
              onMove={(id, start) => edit({ op: "move", clip: id, start })}
              onTrack={(id, patch) => edit({ op: "set", target: id, fields: patch })}
            />
          </div>
        </main>
        <aside>
          <Show when={error()}>
            <p class="error" role="alert">{error()}</p>
          </Show>
          <Show when={problems().length}>
            <section>
              <h2>Problems</h2>
              <For each={problems()}>{(p) => <p class="error">{p.path} {p.message}</p>}</For>
            </section>
          </Show>
          <Switch>
            <Match when={clip()}>
              {(c) => (
                <ClipPanel
                  clip={c()}
                  fields={fields().clips[c().type] ?? {}}
                  player={player()}
                  frame={frame()}
                  onChange={(name, value) => edit({ op: "set", target: c().id, fields: { [name]: value ?? null } }, `${c().id}.${name}`)}
                  onSplit={() => edit({ op: "split", clip: c().id, at: frame() })}
                  onDelete={() => (edit({ op: "remove", id: c().id }), setSelected(undefined))}
                />
              )}
            </Match>
            <Match when={!clip()}>
              <ProjectPanel project={project()!} file={state()!.file} fields={fields().project} frame={frame()} edit={edit} onAdded={setSelected} />
            </Match>
          </Switch>
          <RenderPanel frames={duration()} />
        </aside>
      </div>
    </Show>
  );
}

function Preview(p: { url: string; width: number; height: number; safeZone: boolean; onLoad: (w: Window) => void }) {
  let box!: HTMLDivElement;
  const [scale, setScale] = createSignal(0.3);
  onMount(() => {
    const fit = new ResizeObserver(([e]) => setScale(Math.min(e.contentRect.width / p.width, e.contentRect.height / p.height)));
    fit.observe(box);
    onCleanup(() => fit.disconnect());
  });
  // Instagram Reels safe zone, scaled from a 1080x1920 frame: top 250, right 200, bottom 440, left 96.
  const k = () => (scale() * p.width) / 1080;
  return (
    <div class="preview" ref={box}>
      <div class="frame" style={{ width: `${p.width * scale()}px`, height: `${p.height * scale()}px` }}>
        <iframe
          title="Player"
          src={p.url}
          width={p.width}
          height={p.height}
          style={{ transform: `scale(${scale()})` }}
          onLoad={(e) => p.onLoad(e.currentTarget.contentWindow!)}
        />
        <Show when={p.safeZone}>
          <div class="safe" style={{ top: `${250 * k()}px`, right: `${200 * k()}px`, bottom: `${440 * k()}px`, left: `${96 * k()}px` }} />
        </Show>
      </div>
    </div>
  );
}

function ClipPanel(p: {
  clip: Clip;
  fields: Fields;
  player?: Window;
  frame: number;
  onChange: (name: string, value: unknown) => void;
  onSplit: () => void;
  onDelete: () => void;
}) {
  const fields = () => p.fields;
  // A scene may export a schema for its props; then each prop gets its own field.
  const propsSchema = () =>
    p.clip.type === "component" ? (p.player?.__karkaComponents?.[`@project/${p.clip.component.replace(/^\.\//, "")}`]?.schema as Record<string, PropSchema> | undefined) : undefined;
  const inside = () => p.frame > p.clip.start && p.frame < p.clip.start + p.clip.duration;
  return (
    <section>
      <header>
        <h2>
          {p.clip.type} · {p.clip.id}
        </h2>
      </header>
      <div class="actions">
        <button disabled={!inside()} title="Split at the playhead (S)" onClick={p.onSplit}>Split at playhead</button>
        <button title="Delete (⌫)" onClick={p.onDelete}>Delete</button>
      </div>
      <For each={Object.keys(fields())}>
        {(name) => (
          <Show
            when={name === "props" && propsSchema()}
            fallback={<Field name={name} schema={fields()[name]} value={(p.clip as unknown as Record<string, unknown>)[name]} onChange={(v) => p.onChange(name, v)} />}
          >
            {(schema) => {
              const props = () => ((p.clip as { props?: Record<string, unknown> }).props ?? {});
              return (
                <fieldset>
                  <legend>Props</legend>
                  <For each={propNames(props(), schema())}>
                    {(prop) => <Field name={`props.${prop}`} schema={schema()[prop]} value={props()[prop]} onChange={(v) => p.onChange("props", { [prop]: v ?? null })} />}
                  </For>
                </fieldset>
              );
            }}
          </Show>
        )}
      </For>
    </section>
  );
}

/** Defaults for a clip added from the Studio: 3 seconds at the playhead. */
const starter = (type: ClipType, fps: number): Record<string, unknown> => {
  const duration = 3 * fps;
  return {
    video: { type: "video", src: "media/clip.mp4", duration },
    image: { type: "image", src: "media/image.png", duration },
    audio: { type: "audio", src: "media/sound.wav", duration },
    text: { type: "text", text: "Text", duration },
    color: { type: "color", color: "#334", duration },
    component: { type: "component", component: "scenes/Scene.tsx", duration },
  }[type];
};

function ProjectPanel(p: {
  project: Project;
  file: string;
  fields: Fields;
  frame: number;
  edit: (change: Edit, key?: string) => Promise<Record<string, unknown> | undefined>;
  onAdded: (id: string) => void;
}) {
  const [type, setType] = createSignal<ClipType>("text");
  const [track, setTrack] = createSignal("");
  const target = () => track() || p.project.tracks.at(-1)?.id || "";
  return (
    <>
      <section>
        <header>
          <h2>Project · {p.file}</h2>
        </header>
        <For each={Object.keys(p.fields)}>
          {(name) => (
            <Field
              name={name}
              schema={p.fields[name]}
              value={(p.project as unknown as Record<string, unknown>)[name]}
              onChange={(v) => p.edit({ op: "set", target: "project", fields: { [name]: v ?? null } }, `project.${name}`)}
            />
          )}
        </For>
      </section>
      <section>
        <header>
          <h2>Add</h2>
        </header>
        <div class="actions">
          <button onClick={() => p.edit({ op: "add-track" })}>+ Track</button>
        </div>
        <Show when={p.project.tracks.length}>
          <div class="add-clip">
            <select aria-label="Clip type" onChange={(e) => setType(e.currentTarget.value as ClipType)}>
              <For each={CLIP_TYPES}>{(t) => <option selected={t === type()}>{t}</option>}</For>
            </select>
            <select aria-label="Track" onChange={(e) => setTrack(e.currentTarget.value)}>
              <For each={[...p.project.tracks].reverse()}>{(t) => <option value={t.id} selected={t.id === target()}>{t.name ?? t.id}</option>}</For>
            </select>
            <button
              onClick={async () => {
                const added = await p.edit({ op: "add", track: target(), fields: { ...starter(type(), p.project.fps), start: p.frame } });
                const id = (added?.clip as { id?: string } | undefined)?.id;
                if (id) p.onAdded(id);
              }}
            >
              Add at playhead
            </button>
          </div>
          <p class="muted">Then set its fields in the inspector. Select a clip to edit it; drag it to move it.</p>
        </Show>
      </section>
    </>
  );
}

type Job =
  | { state: "idle" }
  | { state: "building" }
  | { state: "rendering"; frame: number; frames: number }
  | { state: "done"; url: string; secs: number }
  | { state: "error"; message: string };

function RenderPanel(p: { frames: number }) {
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
      const res = await fetch("/api/render", { method: "POST" });
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
          <p class="muted">Preparing media and building the player…</p>
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
                Done in {j().secs.toFixed(1)} s · <a href={j().url} target="_blank">{j().url.split("?")[0].slice(1)}</a>
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
