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

declare global {
  interface Window {
    __karka?: Config;
    __setFrame?: (n: number) => unknown;
  }
}

/** Set by `karka render`. Outside karka (a plain browser), these defaults apply. */
export const config: Config = window.__karka ?? { props: {}, frames: 300, fps: 30, width: 1080, height: 1920 };

/** `defaults`, overridden by the props given on the command line. */
export function getProps<P extends object>(defaults: P): P {
  return { ...defaults, ...config.props };
}

const [frame, setFrame] = createSignal(0);
// The engine calls this for each frame. Solid updates the DOM synchronously, before it paints.
window.__setFrame = (n) => void setFrame(n);

/** Frame where the enclosing `Sequence` starts (0 at the top). */
const SequenceStart = createContext(0);

/** The current frame, counted from the start of the enclosing `Sequence`. */
export function useFrame(): Accessor<number> {
  const start = useContext(SequenceStart);
  return () => frame() - start;
}

/** Shows `children` from frame `from` (relative to the enclosing Sequence) for `duration` frames (default: to the end). */
export function Sequence(props: { from: number; duration?: number; children: JSX.Element }) {
  const start = useContext(SequenceStart) + props.from;
  const shown = () => frame() >= start && frame() < start + (props.duration ?? Infinity);
  return (
    <SequenceStart.Provider value={start}>
      <Show when={shown()}>{props.children}</Show>
    </SequenceStart.Provider>
  );
}

/** The Instagram Reels safe zone of the 1080x1920 frame. Everything the viewer must read goes inside; only full-bleed backgrounds go outside. */
export function SafeArea(props: { children: JSX.Element }) {
  return <div style={{ position: "absolute", top: "250px", right: "200px", bottom: "440px", left: "96px" }}>{props.children}</div>;
}
