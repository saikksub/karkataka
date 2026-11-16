import type { Clip, Project } from "@karkataka/player/project";
import { createSignal, For, Show } from "solid-js";

const label = (c: Clip) =>
  c.type === "text" ? c.text : c.type === "color" ? c.color : c.type === "component" ? c.component.split("/").pop()! : c.src.split("/").pop()!;

/**
 * Tracks as rows, top layer first (as in an editor: the last track draws over the others). Click or
 * drag the ruler to seek; click a clip to select it, drag it to move it.
 */
export function Timeline(p: {
  project: Project;
  duration: number;
  fps: number;
  frame: number;
  selected?: string;
  onSeek: (frame: number) => void;
  onSelect: (id: string | undefined) => void;
  onMove: (id: string, start: number) => void;
  onTrack: (id: string, patch: { hidden?: boolean; muted?: boolean }) => void;
}) {
  let lanes!: HTMLDivElement;
  const [drag, setDrag] = createSignal<{ id: string; by: number }>();
  const pct = (frames: number) => `${(100 * frames) / p.duration}%`;
  const frameAt = (clientX: number) => {
    const r = lanes.getBoundingClientRect();
    return Math.round(((clientX - r.left) / r.width) * p.duration);
  };
  const seek = (e: PointerEvent) => {
    p.onSeek(frameAt(e.clientX));
    const move = (m: PointerEvent) => p.onSeek(frameAt(m.clientX));
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", () => window.removeEventListener("pointermove", move), { once: true });
  };
  const grab = (e: PointerEvent, clip: Clip) => {
    e.stopPropagation();
    const from = e.clientX;
    const width = lanes.getBoundingClientRect().width;
    let by = 0;
    const move = (m: PointerEvent) => {
      by = Math.round(((m.clientX - from) / width) * p.duration);
      by = Math.max(by, -clip.start);
      setDrag({ id: clip.id, by });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener(
      "pointerup",
      () => {
        window.removeEventListener("pointermove", move);
        setDrag(undefined);
        if (by === 0) p.onSelect(clip.id);
        else p.onMove(clip.id, clip.start + by);
      },
      { once: true },
    );
  };
  const ticks = () => {
    // About 8 labels, at whole seconds that read well.
    const seconds = p.duration / p.fps;
    const step = [0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300].find((s) => seconds / s <= 8) ?? 600;
    return Array.from({ length: Math.floor(seconds / step) + 1 }, (_, i) => i * step);
  };
  return (
    <div class="tracks">
      <div class="row ruler-row">
        <div class="head" />
        <div class="lane ruler" ref={lanes} onPointerDown={seek}>
          <For each={ticks()}>{(s) => <span class="tick" style={{ left: pct(s * p.fps) }}>{s}s</span>}</For>
          <div class="playhead" style={{ left: pct(p.frame) }} />
        </div>
      </div>
      <For each={[...p.project.tracks].reverse()}>
        {(track) => (
          <div class="row" classList={{ hidden: track.hidden, muted: track.muted }}>
            <div class="head">
              <span class="name" title={track.id}>{track.name ?? track.id}</span>
              <button title={track.hidden ? "Show" : "Hide"} classList={{ off: track.hidden }} onClick={() => p.onTrack(track.id, { hidden: !track.hidden })}>👁</button>
              <button title={track.muted ? "Unmute" : "Mute"} classList={{ off: track.muted }} onClick={() => p.onTrack(track.id, { muted: !track.muted })}>♪</button>
            </div>
            <div class="lane" onPointerDown={(e) => (p.onSelect(undefined), seek(e))}>
              <For each={track.clips}>
                {(clip) => (
                  <div
                    class={`clip ${clip.type}`}
                    classList={{ selected: clip.id === p.selected }}
                    title={`${clip.id} · ${clip.type} · frames ${clip.start}–${clip.start + clip.duration - 1}`}
                    style={{ left: pct(clip.start + (drag()?.id === clip.id ? drag()!.by : 0)), width: pct(clip.duration) }}
                    onPointerDown={(e) => grab(e, clip)}
                  >
                    <span>{label(clip)}</span>
                  </div>
                )}
              </For>
              <div class="playhead" style={{ left: pct(p.frame) }} />
            </div>
          </div>
        )}
      </For>
      <Show when={p.project.tracks.length === 0}>
        <p class="muted empty">No tracks yet. Add one in the inspector, or with `karkataka add-track`.</p>
      </Show>
    </div>
  );
}
