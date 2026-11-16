import { expect, test } from "vitest";
import { clampFrame, playhead } from "./timeline.ts";

test("playhead advances with time at the frame rate", () => {
  expect(playhead(0, 0, 30, 90, false)).toEqual({ frame: 0, ended: false });
  expect(playhead(0, 1000, 30, 90, false)).toEqual({ frame: 30, ended: false });
  expect(playhead(10, 33, 30, 90, false)).toEqual({ frame: 10, ended: false }); // 0.99 of a frame
  expect(playhead(10, 34, 30, 90, false)).toEqual({ frame: 11, ended: false });
});

test("at the end, playback stops on the last frame or loops", () => {
  expect(playhead(80, 1000, 30, 90, false)).toEqual({ frame: 89, ended: true });
  expect(playhead(80, 1000, 30, 90, true)).toEqual({ frame: 20, ended: false });
  expect(playhead(0, 7000, 30, 90, true)).toEqual({ frame: 30, ended: false }); // wrapped twice
});

test("clampFrame keeps frames in range", () => {
  expect(clampFrame(-3, 90)).toBe(0);
  expect(clampFrame(200, 90)).toBe(89);
  expect(clampFrame(4.6, 90)).toBe(5);
  expect(clampFrame(5, 0)).toBe(0);
});
