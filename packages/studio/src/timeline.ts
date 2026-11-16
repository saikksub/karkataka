/** Where playback is `elapsedMs` after it started at frame `from`, and whether it stopped at the end. */
export function playhead(from: number, elapsedMs: number, fps: number, frames: number, loop: boolean): { frame: number; ended: boolean } {
  const frame = from + Math.floor((elapsedMs * fps) / 1000);
  if (frame < frames) return { frame, ended: false };
  return loop ? { frame: frame % frames, ended: false } : { frame: frames - 1, ended: true };
}

/** `n` kept within the frames `0..frames-1`. */
export function clampFrame(n: number, frames: number): number {
  return Math.min(Math.max(0, Math.round(n)), Math.max(0, frames - 1));
}
