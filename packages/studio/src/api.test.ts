import { expect, test } from "vitest";
import type { Project } from "@karkataka/player/project";
import { durationOf, findClip, forPlayer, scenesOf } from "./api.ts";

const project: Project = {
  version: 1,
  width: 1920,
  height: 1080,
  fps: 30,
  tracks: [
    { id: "V1", clips: [{ id: "v", type: "video", src: "media/my clip.mp4", start: 0, duration: 60 }, { id: "s", type: "component", component: "./scenes/Intro.tsx", start: 60, duration: 30 }] },
    { id: "T1", clips: [{ id: "t", type: "text", text: "Hi", start: 10, duration: 20 }, { id: "u", type: "image", src: "https://x/y.png", start: 40, duration: 5 }, { id: "s2", type: "component", component: "scenes/Intro.tsx", start: 50, duration: 5 }] },
  ],
};

test("length is the duration, or where the last clip ends", () => {
  expect(durationOf(project)).toBe(90);
  expect(durationOf({ ...project, duration: 300 })).toBe(300);
  expect(durationOf({ ...project, tracks: [] })).toBe(1);
});

test("clips are found by id", () => {
  expect(findClip(project, "t")?.type).toBe("text");
  expect(findClip(project, "nope")).toBeUndefined();
  expect(findClip(project, undefined)).toBeUndefined();
});

test("the player gets media URLs under /files/ and scene import keys", () => {
  const p = forPlayer(project);
  expect(p.duration).toBe(90);
  expect(p.tracks[0].clips[0]).toMatchObject({ url: "/files/media/my%20clip.mp4" });
  expect(p.tracks[0].clips[1]).toMatchObject({ module: "@project/scenes/Intro.tsx" });
  expect(p.tracks[1].clips[0]).not.toHaveProperty("url");
  expect(p.tracks[1].clips[1]).toMatchObject({ url: "https://x/y.png" });
  expect(scenesOf(project)).toBe("scenes/Intro.tsx");
});
