/// <reference types="vite/client" />
import type { PropSchema } from "@karkataka/runtime";
import type { PlayerProject } from "@karkataka/sdk";
import {
  addClip, addTrack, CLIP_FIELDS, CLIP_TYPES, durationOf, find, moveClip, PROJECT_FIELDS, remove, splitClip, updateClip, updateProject, updateTrack,
  type Clip, type ClipType, type NewClip, type Problem, type Project,
} from "@karkataka/sdk/edit";
import { createEffect, createMemo, createSignal, For, Match, on, onCleanup, onMount, Show, Switch, untrack } from "solid-js";
import { propNames } from "../fields.ts";
import type { ProjectState, RenderEvent } from "../plugin.ts";
import { clampFrame, playhead } from "../timeline.ts";
import { Field } from "./Field.tsx";
import { Timeline } from "./Timeline.tsx";

/** The project with media URLs this dev server serves. */
function forPlayer(project: Project, dir: string): PlayerProject {
  const url = (src: string) => (src.includes("://") ? src : `/@fs${src.startsWith("/") ? src : `${dir}/${src}`}`);
  const clips = (c: Clip) => (c.type === "video" || c.type === "image" || c.type === "audio" ? { ...c, url: url(c.src) } : c);
  return { ...project, duration: durationOf(project), tracks: project.tracks.map((t) => ({ ...t, clips: t.clips.map(clips) })) };
}

export function App() {
  const [state, setState] = createSignal<Omit<ProjectState, "problems">>();
  const [problems, setProblems] = createSignal<Problem[]>([]);
  const [error, setError] = createSignal("");
  const [selected, setSelected] = createSignal<string>();
  const [frame, setFrame] = createSignal(0);
  const [playing, setPlaying] = createSignal(false);
  const [loop, setLoop] = createSignal(true);
  const [safeZone, setSafeZone] = createSignal(true);
  const [player, setPlayer] = createSignal<Window>();
  const project = () => state()?.project;
  const duration = () => (project() ? durationOf(project()!) : 1);
  const fps = () => project()?.fps ?? 30;

  // Undo history. Edits to the same field in quick succession (typing) count as one step.
  const past: Project[] = [];
  let future: Project[] = [];
  let lastKey = "";
  let lastAt = 0;
  let saved = "";
  let saving: ReturnType<typeof setTimeout>;

  async function load() {
    const res = await fetch("/__karka/project");
    const body = await res.json();
    if (!res.ok) return setError(body.error);
    const s = body as ProjectState;
    setProblems(s.problems);
    // Our own save coming back: nothing new.
    if (JSON.stringify(s.project) === saved) return;
    if (project()) past.push(project()!);
    saved = JSON.stringify(s.project);
    setState({ project: s.project, dir: s.dir, file: s.file });
  }

  function commit(next: Project) {
    setState((s) => ({ ...s!, project: next }));
    clearTimeout(saving);
    saving = setTimeout(async () => {
      saved = JSON.stringify(next);
      const res = await fetch("/__karka/project", { method: "PUT", body: saved });
      const body = await res.json();
      setProblems(body.problems ?? []);
      if (!res.ok) setError(body.error);
    }, 250);
  }

  /** Apply an edit (an SDK operation); show why when it is refused. */
  function edit(change: (p: Project) => Project, key = "") {
    try {
      const next = change(project()!);
      if (key === "" || key !== lastKey || Date.now() - lastAt > 1000) past.push(project()!);
      [lastKey, lastAt, future] = [key, Date.now(), []];
      setError("");
      commit(next);
    } catch (e) {
      setError((e as Error).message);
    }
  }
  const undo = () => past.length && (future.push(project()!), commit(past.pop()!));
  const redo = () => future.length && (past.push(project()!), commit(future.pop()!));

  onMount(() => {
    load();
    // The file changed on disk (an agent's edit, or yours in an editor).
    import.meta.hot?.on("karka:project", load);
  });

  // The player page: fps and size are read once when it loads, so a change reloads it.
  const playerUrl = createMemo(() => {
    const p = project();
    return p && `/player?karka=${encodeURIComponent(JSON.stringify({ fps: p.fps, width: p.width, height: p.height, frames: durationOf(p), props: {} }))}`;
  });
  createEffect(() => {
    const p = project();
    const w = player();
    if (p && w?.__karkaSetProject) {
      w.__karkaSetProject(forPlayer(p, state()!.dir));
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
        Delete: () => selected() && (edit((p) => remove(p, selected()!).project), setSelected(undefined)),
        Backspace: () => selected() && (edit((p) => remove(p, selected()!).project), setSelected(undefined)),
        s: () => selected() && edit((p) => splitClip(p, selected()!, frame()).project),
      };
      if (actions[e.key]) {
        e.preventDefault();
        actions[e.key]();
      }
    };
    window.addEventListener("keydown", keys);
    onCleanup(() => window.removeEventListener("keydown", keys));
  });

  const clip = () => {
    const id = selected();
    if (!id || !project()) return undefined;
    try {
      return find(project()!, id).clip;
    } catch {
      return undefined;
    }
  };
  const portrait = () => project() && project()!.width * 16 === project()!.height * 9;

  return (
    <Show when={project()} fallback={<p class="muted loading">{error() || "Loading the project…"}</p>}>
      <div class="studio">
        <main>
          <div class="top">
            <Preview url={playerUrl()!} width={project()!.width} height={project()!.height} safeZone={Boolean(portrait()) && safeZone()} onLoad={(w) => setPlayer(() => w)} />
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
              onMove={(id, start) => edit((p) => moveClip(p, id, { start }).project)}
              onTrack={(id, patch) => edit((p) => updateTrack(p, id, patch).project)}
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
                  player={player()}
                  frame={frame()}
                  onChange={(name, value) => edit((p) => updateClip(p, c().id, { [name]: value ?? null }).project, `${c().id}.${name}`)}
                  onSplit={() => edit((p) => splitClip(p, c().id, frame()).project)}
                  onDelete={() => (edit((p) => remove(p, c().id).project), setSelected(undefined))}
                />
              )}
            </Match>
            <Match when={!clip()}>
              <ProjectPanel project={project()!} file={state()!.file} frame={frame()} edit={edit} onAdded={setSelected} />
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
  player?: Window;
  frame: number;
  onChange: (name: string, value: unknown) => void;
  onSplit: () => void;
  onDelete: () => void;
}) {
  const fields = () => CLIP_FIELDS[p.clip.type];
  // A component may export a schema for its props; then each prop gets its own field.
  const propsSchema = () => (p.clip.type === "component" ? (p.player?.__karkaComponents?.[p.clip.component]?.schema as Record<string, PropSchema> | undefined) : undefined);
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
const starter = (type: ClipType, fps: number): NewClip => {
  const duration = 3 * fps;
  return {
    video: { type: "video", src: "media/clip.mp4", duration },
    image: { type: "image", src: "media/image.png", duration },
    audio: { type: "audio", src: "media/sound.wav", duration },
    text: { type: "text", text: "Text", duration },
    color: { type: "color", color: "#334", duration },
    component: { type: "component", component: "src/Component.tsx", duration },
  }[type] as NewClip;
};

function ProjectPanel(p: { project: Project; file: string; frame: number; edit: (change: (p: Project) => Project, key?: string) => void; onAdded: (id: string) => void }) {
  const [type, setType] = createSignal<ClipType>("text");
  const [track, setTrack] = createSignal("");
  const target = () => track() || p.project.tracks.at(-1)?.id || "";
  return (
    <>
      <section>
        <header>
          <h2>Project · {p.file}</h2>
        </header>
        <For each={Object.keys(PROJECT_FIELDS)}>
          {(name) => (
            <Field
              name={name}
              schema={PROJECT_FIELDS[name]}
              value={(p.project as unknown as Record<string, unknown>)[name]}
              onChange={(v) => p.edit((pr) => updateProject(pr, { [name]: v ?? null }), `project.${name}`)}
            />
          )}
        </For>
      </section>
      <section>
        <header>
          <h2>Add</h2>
        </header>
        <div class="actions">
          <button onClick={() => p.edit((pr) => addTrack(pr).project)}>+ Track</button>
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
              onClick={() =>
                p.edit((pr) => {
                  const added = addClip(pr, target(), { ...starter(type(), pr.fps), start: p.frame } as NewClip);
                  queueMicrotask(() => p.onAdded(added.clip.id));
                  return added.project;
                })
              }
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
      const res = await fetch("/__karka/render", { method: "POST" });
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
