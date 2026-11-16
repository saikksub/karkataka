import { expect, test } from "vitest";
import { addClip, addTrack, createProject, EditError, find, moveClip, remove, splitClip, updateClip, updateProject, updateTrack } from "./ops.ts";
import type { Project } from "./project.ts";

function edit() {
  let p = createProject();
  p = addTrack(p, { id: "V1" }).project;
  p = addTrack(p, { name: "Titles" }).project;
  p = addClip(p, "V1", { type: "video", src: "a.mp4", duration: 90, trim: 30, fadeIn: 10, fadeOut: 10 }).project;
  return p;
}

test("createProject defaults to 1920x1080 at 30 fps and refuses bad settings", () => {
  expect(createProject()).toEqual({ version: 1, width: 1920, height: 1080, fps: 30, tracks: [] });
  expect(createProject({ width: 1080, height: 1920, background: "#111" })).toMatchObject({ width: 1080, height: 1920, background: "#111" });
  expect(() => createProject({ width: 1081 })).toThrow("width must be an even whole number");
});

test("tracks and clips get ids; a clip with no start goes after the track's last clip", () => {
  let p = edit();
  expect(p.tracks.map((t) => [t.id, t.name])).toEqual([["V1", undefined], ["track-1", "Titles"]]);
  const second = addClip(p, "V1", { type: "color", color: "red", duration: 10 });
  expect(second.clip).toEqual({ type: "color", color: "red", duration: 10, id: "color-1", start: 90 });
  p = addClip(second.project, "V1", { type: "text", text: "x", duration: 5, start: 200, id: "title" }).project;
  expect(p.tracks[0].clips.map((c) => `${c.id}@${c.start}`)).toEqual(["video-1@0", "color-1@90", "title@200"]);
  // A track goes at an index, or on top.
  expect(addTrack(p, { index: 0 }).project.tracks.map((t) => t.id)).toEqual(["track-2", "V1", "track-1"]);
});

test("edits that would break the project are refused, and the old project is left alone", () => {
  const p = edit();
  const before = structuredClone(p);
  expect(() => addClip(p, "V1", { type: "text", text: "x", duration: 10, start: 50 })).toThrow(/overlap/);
  expect(() => addClip(p, "nope", { type: "text", text: "x", duration: 10 })).toThrow('no track "nope" (tracks: V1, track-1)');
  expect(() => addClip(p, "V1", { type: "text", text: "x", duration: 0 })).toThrow(/duration must be/);
  expect(() => updateClip(p, "video-1", { size: 3 })).toThrow('"size" is not a video clip field');
  expect(() => updateClip(p, "video-1", { type: "image" })).toThrow("id and type cannot change");
  expect(() => updateClip(p, "nope", {})).toThrow('no clip "nope" (clips: video-1)');
  expect(() => updateProject(p, { fps: 0 })).toThrow(/fps/);
  expect(() => updateProject(p, { name: "x" })).toThrow('"name" is not a project setting');
  expect(() => updateTrack(p, "V1", { color: "red" } as never)).toThrow('"color" is not a track setting');
  expect(p).toEqual(before);
  try {
    addClip(p, "V1", { type: "text", text: "x", duration: 10, start: 50 });
  } catch (e) {
    expect(e).toBeInstanceOf(EditError);
    expect((e as EditError).problems[0].path).toBe("tracks[0]");
  }
});

test("updateClip merges objects one level and removes null fields", () => {
  let p = addClip(edit(), "track-1", { type: "text", text: "Hi", duration: 30, style: { fontSize: 80, color: "red" }, id: "t" }).project;
  p = updateClip(p, "t", { style: { color: "blue", fontSize: null }, text: "Hello", fadeIn: 5 }).project;
  expect(find(p, "t").clip).toMatchObject({ text: "Hello", style: { color: "blue" }, fadeIn: 5 });
  p = updateClip(p, "t", { fadeIn: null, style: null }).project;
  expect(find(p, "t").clip).not.toHaveProperty("fadeIn");
  expect(find(p, "t").clip).not.toHaveProperty("style");
  // A change of start keeps the track in time order.
  p = addClip(p, "track-1", { type: "text", text: "2", duration: 10, id: "u" }).project;
  p = updateClip(p, "t", { start: 100 }).project;
  expect(p.tracks[1].clips.map((c) => c.id)).toEqual(["u", "t"]);
});

test("moveClip changes start and track", () => {
  let p = moveClip(edit(), "video-1", { start: 12 }).project;
  expect(find(p, "video-1").clip.start).toBe(12);
  p = moveClip(p, "video-1", { track: "track-1", start: 0 }).project;
  expect(find(p, "video-1").track.id).toBe("track-1");
  expect(p.tracks[0].clips).toEqual([]);
  expect(() => moveClip(p, "video-1", { track: "nope" })).toThrow(/no track/);
});

test("splitClip cuts a clip; the second part plays on from where the first stopped", () => {
  const { project, clips } = splitClip(edit(), "video-1", 40);
  expect(clips[0]).toMatchObject({ id: "video-1", start: 0, duration: 40, trim: 30, fadeIn: 10 });
  expect(clips[0]).not.toHaveProperty("fadeOut");
  expect(clips[1]).toMatchObject({ id: "video-2", start: 40, duration: 50, trim: 70, fadeOut: 10 });
  expect(clips[1]).not.toHaveProperty("fadeIn");
  expect(project.tracks[0].clips.map((c) => c.id)).toEqual(["video-1", "video-2"]);
  // Text has no trim.
  const text = addClip(edit(), "track-1", { type: "text", text: "x", duration: 10, id: "t" }).project;
  expect(splitClip(text, "t", 5).clips[1]).not.toHaveProperty("trim");
  expect(() => splitClip(edit(), "video-1", 0)).toThrow("split frame must be inside the clip, from 1 to 89 (got 0)");
  expect(() => splitClip(edit(), "video-1", 90)).toThrow(/inside the clip/);
});

test("remove takes out a clip or a whole track", () => {
  const p = edit();
  expect(remove(p, "video-1").project.tracks[0].clips).toEqual([]);
  expect(remove(p, "V1").project.tracks.map((t) => t.id)).toEqual(["track-1"]);
  expect(() => remove(p, "nope")).toThrow(/no clip "nope"/);
});

test("project and track settings change; null removes", () => {
  let p: Project = updateProject(edit(), { width: 1080, height: 1920, duration: 300, background: "#222" });
  expect(p).toMatchObject({ width: 1080, height: 1920, duration: 300, background: "#222" });
  p = updateProject(p, { duration: null });
  expect(p).not.toHaveProperty("duration");
  const { track } = updateTrack(p, "V1", { name: "Main", muted: true });
  expect(track).toEqual({ id: "V1", name: "Main", muted: true, clips: expect.any(Array) });
  expect(updateTrack(p, "V1", { muted: null } as never).track).not.toHaveProperty("muted");
});
