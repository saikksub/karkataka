export interface InterpolateOptions {
  /** Applied to the 0..1 progress within each segment. */
  easing?: (t: number) => number;
  /** Hold the end values outside `inputRange` (default). `false` extends the end segments. */
  clamp?: boolean;
}

/** Map `input` from `inputRange` to `outputRange`, piecewise linear. Ranges have the same length (2 or more) and `inputRange` increases. */
export function interpolate(
  input: number,
  inputRange: readonly number[],
  outputRange: readonly number[],
  { easing = (t) => t, clamp = true }: InterpolateOptions = {},
): number {
  if (inputRange.length < 2 || inputRange.length !== outputRange.length) {
    throw new Error("interpolate: inputRange and outputRange need the same length, at least 2");
  }
  for (let i = 1; i < inputRange.length; i++) {
    if (!(inputRange[i] > inputRange[i - 1])) throw new Error("interpolate: inputRange must increase");
  }
  let i = 0;
  while (i < inputRange.length - 2 && input >= inputRange[i + 1]) i++;
  let t = (input - inputRange[i]) / (inputRange[i + 1] - inputRange[i]);
  if (clamp) t = Math.min(1, Math.max(0, t));
  return outputRange[i] + (outputRange[i + 1] - outputRange[i]) * easing(t);
}

export interface SpringOptions {
  /** Frames since the spring started. At 0 or below it rests at `from`, so `frame - delay` delays it. */
  frame: number;
  fps?: number;
  from?: number;
  to?: number;
  stiffness?: number;
  damping?: number;
  mass?: number;
}

/** A spring released at rest from `from` toward `to`, `frame` frames ago. Exact (no stepping), so any frame can be drawn on its own. */
export function spring({ frame, fps = 30, from = 0, to = 1, stiffness = 100, damping = 10, mass = 1 }: SpringOptions): number {
  if (frame <= 0) return from;
  const t = frame / fps;
  const w0 = Math.sqrt(stiffness / mass);
  const z = damping / (2 * Math.sqrt(stiffness * mass));
  // Distance left to `to`, as a fraction of the whole move: -1 at the start, moving at speed 0.
  let x: number;
  if (z < 1) {
    const wd = w0 * Math.sqrt(1 - z * z);
    x = -Math.exp(-z * w0 * t) * (Math.cos(wd * t) + ((z * w0) / wd) * Math.sin(wd * t));
  } else if (z === 1) {
    x = -Math.exp(-w0 * t) * (1 + w0 * t);
  } else {
    const s = w0 * Math.sqrt(z * z - 1);
    const r1 = -z * w0 + s;
    const r2 = -z * w0 - s;
    const c2 = r1 / (r2 - r1);
    x = (-1 - c2) * Math.exp(r1 * t) + c2 * Math.exp(r2 * t);
  }
  return from + (to - from) * (1 + x);
}
