import { AssetBase, Audio, interpolate, isReporting, Sequence, useDuration, useFrame, waitFor } from "@karkataka/runtime";
import { createRenderEffect, For, Match, Show, Switch, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import type { TextStyle } from "./project.ts";
import type { ComponentModule, PlayerClip, PlayerProject } from "./types.ts";

/** Draws `project` at the runtime's current frame. Track 0 is the bottom layer. */
export function Player(p: { project: PlayerProject; components: Record<string, ComponentModule> }) {
  return (
    <div style={{ position: "relative", width: `${p.project.width}px`, height: `${p.project.height}px`, overflow: "hidden", background: p.project.background ?? "#000" }}>
      <For each={p.project.tracks}>
        {(track) => (
          <For each={track.clips}>
            {(clip) => (
              <Sequence from={clip.start} duration={clip.duration}>
                <Show when={!track.muted && clip.path && (clip.type === "audio" || (clip.type === "video" && clip.audio))}>
                  <Audio src={clip.path!} trim={[(clip as { trim?: number }).trim ?? 0]} volume={(clip as { volume?: number }).volume ?? 1} />
                </Show>
                <Show when={!track.hidden && clip.type !== "audio"}>
                  <Visual clip={clip} project={p.project} components={p.components} />
                </Show>
              </Sequence>
            )}
          </For>
        )}
      </For>
    </div>
  );
}

/** A visual clip in its box, with its opacity and fades. */
function Visual(p: { clip: PlayerClip; project: PlayerProject; components: Record<string, ComponentModule> }) {
  const f = useFrame();
  const duration = useDuration();
  const box = () => p.clip.box ?? { x: 0, y: 0, width: p.project.width, height: p.project.height };
  const opacity = () => {
    const { fadeIn = 0, fadeOut = 0, opacity = 1 } = p.clip;
    const fin = fadeIn > 0 ? interpolate(f(), [0, fadeIn], [0, 1]) : 1;
    const fout = fadeOut > 0 ? interpolate(f(), [duration - fadeOut, duration], [1, 0]) : 1;
    return opacity * fin * fout;
  };
  return (
    <div
      data-clip={p.clip.id}
      style={{
        position: "absolute",
        left: `${box().x}px`,
        top: `${box().y}px`,
        width: `${box().width}px`,
        height: `${box().height}px`,
        opacity: `${opacity()}`,
        overflow: "hidden",
      }}
    >
      <Switch>
        <Match when={p.clip.type === "video" && p.clip}>{(c) => (c().frames ? <VideoFrames clip={c()} /> : <VideoSeek clip={c()} fps={p.project.fps} />)}</Match>
        <Match when={p.clip.type === "image" && p.clip}>{(c) => <Image clip={c()} />}</Match>
        <Match when={p.clip.type === "text" && (p.clip as PlayerClip & { type: "text" })}>{(c) => <Text text={c().text} style={c().style} />}</Match>
        <Match when={p.clip.type === "color" && (p.clip as PlayerClip & { type: "color" })}>{(c) => <div style={{ position: "absolute", inset: "0", background: c().color }} />}</Match>
        <Match when={p.clip.type === "component" && (p.clip as PlayerClip & { type: "component" })}>
          {(c) => {
            const module = () => p.components[c().module ?? ""];
            return (
              <Show when={module()?.default} fallback={<Missing what={`component ${c().component}`} />}>
                <AssetBase.Provider value={p.project.base ?? ""}>
                  <Dynamic component={module()!.default as (props: Record<string, unknown>) => JSX.Element} {...(c().props ?? {})} />
                </AssetBase.Provider>
              </Show>
            );
          }}
        </Match>
      </Switch>
    </div>
  );
}

/**
 * Resolves once `img` has loaded and decoded. Not `img.decode()` alone: Chrome rejects that on an
 * image created a moment ago ("EncodingError"), and the frame would be captured before it shows.
 */
function loaded(img: HTMLImageElement): Promise<void> {
  return new Promise((resolve, reject) => {
    const done = () => img.decode().catch(() => {}).then(() => resolve());
    if (img.complete && img.naturalWidth > 0) return done();
    img.addEventListener("load", done, { once: true });
    img.addEventListener("error", () => reject(new Error(`cannot load image ${img.src}`)), { once: true });
  });
}

const fill = (fit: string | undefined): JSX.CSSProperties => ({ width: "100%", height: "100%", display: "block", "object-fit": (fit ?? "cover") as JSX.CSSProperties["object-fit"] });

/** Render: the clip's frames, extracted beforehand as images; each frame waits until its image has decoded. */
function VideoFrames(p: { clip: PlayerClip & { type: "video" } }) {
  const f = useFrame();
  const img = (<img alt="" style={fill(p.clip.fit)} />) as HTMLImageElement;
  createRenderEffect(() => {
    const { url, first, count } = p.clip.frames!;
    const i = Math.min(Math.max(f() - first, 0), count - 1);
    const src = `${url}${String(i + 1).padStart(6, "0")}.jpg`;
    if (isReporting() || count === 0 || img.getAttribute("src") === src) return;
    img.src = src;
    waitFor(loaded(img));
  });
  return img;
}

/** Studio preview: seek a <video> to the frame (the browser decodes the file itself). */
function VideoSeek(p: { clip: PlayerClip & { type: "video" }; fps: number }) {
  const f = useFrame();
  const video = (<video src={p.clip.url} muted preload="auto" playsinline style={fill(p.clip.fit)} />) as HTMLVideoElement;
  createRenderEffect(() => {
    // The middle of the frame, so rounding never lands on the frame before.
    const time = ((p.clip.trim ?? 0) + f() + 0.5) / p.fps;
    if (Math.abs(video.currentTime - time) < 0.5 / p.fps) return;
    video.currentTime = time;
    // At most 2 s, so a file the browser cannot play does not hold the preview.
    waitFor(Promise.race([new Promise((r) => video.addEventListener("seeked", r, { once: true })), new Promise((r) => setTimeout(r, 2000))]));
  });
  return video;
}

function Image(p: { clip: PlayerClip & { type: "image" } }) {
  const img = (<img alt="" src={p.clip.url} style={fill(p.clip.fit)} />) as HTMLImageElement;
  if (!isReporting()) waitFor(loaded(img));
  return img;
}

function Text(p: { text: string; style?: TextStyle }) {
  const s = () => p.style ?? {};
  const place = { top: "flex-start", center: "center", bottom: "flex-end" } as const;
  return (
    <div
      style={{
        position: "absolute",
        inset: "0",
        display: "flex",
        "flex-direction": "column",
        "justify-content": place[s().verticalAlign ?? "center"],
        "text-align": s().align ?? "center",
        padding: `${s().padding ?? 0}px`,
        background: s().background ?? "transparent",
        color: s().color ?? "#fff",
        "font-family": s().fontFamily ?? "-apple-system, Helvetica, Arial, sans-serif",
        "font-size": `${s().fontSize ?? 64}px`,
        "font-weight": String(s().fontWeight ?? 700),
        "line-height": String(s().lineHeight ?? 1.2),
        "white-space": "pre-wrap",
      }}
    >
      {p.text}
    </div>
  );
}

/** Shown in the frame, so a broken clip is seen in the preview and in stills, not silently left out. */
function Missing(p: { what: string }) {
  return <div style={{ position: "absolute", inset: "0", display: "grid", "place-items": "center", background: "#400", color: "#fff", font: "bold 32px sans-serif" }}>missing {p.what}</div>;
}
