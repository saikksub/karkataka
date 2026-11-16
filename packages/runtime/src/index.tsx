import { createContext, createSignal, Show, useContext, type Accessor, type JSX } from "solid-js";

export { interpolate, spring, type InterpolateOptions, type SpringOptions } from "./math.ts";

export interface Config {
  /** From `karka render --props '<json>'`. */
  props: Record<string, unknown>;
  /** From `karka render --frames`. */
  frames: number;
  fps: number;
  width: number;
  height: number;
}

/** How the Studio shows one prop. JSON Schema keywords, plus `multiline`. */
export interface PropSchema {
  type?: "string" | "number" | "integer" | "boolean" | "array" | "object";
  title?: string;
  description?: string;
  enum?: readonly (string | number)[];
  minimum?: number;
  maximum?: number;
  /** For strings: a text area instead of one line. */
  multiline?: boolean;
  /** For arrays. A list of strings gets one field per item; anything else is edited as JSON. */
  items?: PropSchema;
}

/** One `PropSchema` per prop. */
export type PropsSchema = Record<string, PropSchema>;

declare global {
  interface Window {
    __karka?: Config;
    /** What the Studio reads from the scene: its default props and their schema. */
    __karkaScene?: { defaults: object; schema?: PropsSchema };
    __setFrame?: (n: number) => unknown;
    __karkaReport?: () => string;
  }
}

/** `{ props, frames }` from the page URL's `?karka=` (JSON), which is how the Studio passes them. */
function fromUrl(): Partial<Config> {
  try {
    return JSON.parse(new URLSearchParams(location.search).get("karka") ?? "{}");
  } catch {
    return {};
  }
}

/** Set by `karka render`. Elsewhere: these defaults, overridden by the Studio's `?karka=`. */
export const config: Config = window.__karka ?? { props: {}, frames: 300, fps: 30, width: 1920, height: 1080, ...fromUrl() };

/** `defaults`, overridden by the props from `karka render --props` or the Studio. `schema` tells the Studio how to edit them. */
export function getProps<P extends object>(defaults: P, schema?: PropsSchema): P {
  window.__karkaScene = { defaults, schema };
  return { ...defaults, ...config.props };
}

const [frame, setFrame] = createSignal(0);
let pending: Promise<unknown>[] = [];
let reporting = false;

/**
 * Make the current frame wait for `promise` before karka captures it: an image decoding, a font
 * loading, data arriving. Call it while drawing the frame. A rejected promise is logged, not fatal.
 */
export function waitFor(promise: Promise<unknown>): void {
  pending.push(promise.catch((e) => console.error("karka: a frame waited for something that failed:", e)));
}

/** True while karka lists the audio clips (it shows every frame once): skip slow work such as loading images. */
export function isReporting(): boolean {
  return reporting;
}

// The engine calls this for each frame. Solid updates the DOM synchronously; then wait for anything
// the frame asked to wait for.
window.__setFrame = (n) => {
  setFrame(n);
  if (pending.length === 0) return;
  const waits = pending;
  pending = [];
  return Promise.all(waits).then(() => undefined);
};

/** Frames where the enclosing `Sequence` starts and ends (the end is not included). */
const Span = createContext({ start: 0, end: Infinity });

/** The current frame, counted from the start of the enclosing `Sequence`. */
export function useFrame(): Accessor<number> {
  const { start } = useContext(Span);
  return () => frame() - start;
}

/** Length of the enclosing `Sequence` in frames (at the top: the whole video). */
export function useDuration(): number {
  const { start, end } = useContext(Span);
  return (Number.isFinite(end) ? end : config.frames) - start;
}

/** Shows `children` from frame `from` (relative to the enclosing Sequence) for `duration` frames (default: to the end). */
export function Sequence(props: { from: number; duration?: number; children: JSX.Element }) {
  const parent = useContext(Span);
  const start = parent.start + props.from;
  const end = Math.min(parent.end, start + (props.duration ?? Infinity));
  return (
    <Span.Provider value={{ start, end }}>
      <Show when={frame() >= start && frame() < end}>{props.children}</Show>
    </Span.Provider>
  );
}

/** An audio clip as karka mixes it. Frames are video frames, except `skip` (source frames cut from its start). */
export interface AudioClip {
  src: string;
  from: number;
  /** `null`: to the end of the video. */
  to: number | null;
  skip: number;
  volume: number;
}

/**
 * Where a relative `<Audio src>` is found: an absolute folder path ending in "/". Without one,
 * karka looks next to the page. The project player sets it to the project's folder.
 */
export const AssetBase = createContext("");

// Keyed by the clip itself: a Sequence that mounts again re-registers the same clip, which must not
// play twice. (So two identical clips at the same frame also play once.)
const clips = new Map<string, AudioClip>();

/**
 * Plays `src` from frame `start` (relative to the enclosing Sequence) until the Sequence ends.
 * `trim={[a, b]}` plays the source from its frame `a` to frame `b` (`b` optional). `volume`: 1 is
 * unchanged. A relative `src` is a file next to the built page (in Vite: put it in `public/`).
 * Draws nothing; `karka render` mixes it into the MP4.
 */
export function Audio(props: { src: string; start?: number; trim?: [number, number?]; volume?: number }) {
  const span = useContext(Span);
  const base = useContext(AssetBase);
  const src = base && !props.src.startsWith("/") && !props.src.includes("://") ? base + props.src : props.src;
  const from = span.start + (props.start ?? 0);
  const [skip = 0, until] = props.trim ?? [];
  const to = Math.min(span.end, until === undefined ? Infinity : from + until - skip);
  const clip: AudioClip = { src, from, to: Number.isFinite(to) ? to : null, skip, volume: props.volume ?? 1 };
  clips.set(JSON.stringify(clip), clip);
  return null;
}

// karka calls this once after load. Clips inside a Sequence register only when it shows, so show
// every frame once first.
window.__karkaReport = () => {
  reporting = true;
  try {
    for (let n = 0; n < config.frames; n++) setFrame(n);
  } finally {
    reporting = false;
    pending = [];
  }
  setFrame(0);
  return JSON.stringify({ audio: [...clips.values()] });
};

/** For 9:16 (1080x1920) phone video: the Instagram Reels safe zone. Everything the viewer must read goes inside; only full-bleed backgrounds go outside. */
export function SafeArea(props: { children: JSX.Element }) {
  return <div style={{ position: "absolute", top: "250px", right: "200px", bottom: "440px", left: "96px" }}>{props.children}</div>;
}
