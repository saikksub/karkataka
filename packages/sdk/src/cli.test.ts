import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, expect, test } from "vitest";
import { assignments, main } from "./cli.ts";

const dir = mkdtempSync(join(tmpdir(), "karkataka-cli-"));
const file = join(dir, "p.json");
afterAll(() => rmSync(dir, { recursive: true, force: true }));
// 2 s of video with sound, and an image.
execFileSync("ffmpeg", ["-v", "error", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25:duration=2", "-f", "lavfi", "-i", "sine=duration=2", "-shortest", join(dir, "clip.mp4")]);
execFileSync("ffmpeg", ["-v", "error", "-f", "lavfi", "-i", "testsrc2=size=64x48", "-frames:v", "1", join(dir, "pic.png")]);

const run = (...args: string[]) => main(args) as Promise<Record<string, any>>;
const saved = () => JSON.parse(readFileSync(file, "utf8"));

test("key=value: JSON values, text otherwise, dotted keys nest", () => {
  expect(assignments(["a=1", "b=true", "c=null", "d=hello world", 'e={"x":1}', "f.g.h=2", "f.g.i=x", "j="])).toEqual({
    a: 1, b: true, c: null, d: "hello world", e: { x: 1 }, f: { g: { h: 2, i: "x" } }, j: "",
  });
  expect(() => assignments(["nokey"])).toThrow('expected key=value, got "nokey"');
  expect(() => assignments(["=1"])).toThrow(/expected key=value/);
});

test("an agent builds a project step by step", async () => {
  expect((await run("new", file, "--width", "1280", "--height", "720", "--fps", "25", "--background", "#111")).project).toMatchObject({ width: 1280, fps: 25, background: "#111" });
  await expect(run("new", file)).rejects.toThrow("exists; pass --force");
  expect((await run("add-track", file, "--id", "V1", "--name", "Video")).track).toEqual({ id: "V1", name: "Video", clips: [] });
  await run("add-track", file, "--id", "T1");
  await run("add-track", file, "--id", "BG", "--index", "0");
  expect(saved().tracks.map((t: { id: string }) => t.id)).toEqual(["BG", "V1", "T1"]);

  // No duration: the rest of the file after the trim (2 s at 25 fps = 50 frames, minus 10).
  const video = await run("add", file, "--track", "V1", "type=video", "src=clip.mp4", "trim=10", "volume=0.5");
  expect(video.clip).toEqual({ type: "video", src: "clip.mp4", trim: 10, volume: 0.5, duration: 40, id: "video-1", start: 0 });
  expect(video.duration).toBe(40);
  expect(video.problems).toEqual([]);
  // Text stays text even when it looks like a number; dotted keys set style fields.
  const title = await run("add", file, "--track", "T1", "type=text", "text=2024", "duration=20", "start=5", "style.fontSize=96", "style.color=red");
  expect(title.clip).toMatchObject({ id: "text-1", text: "2024", start: 5, style: { fontSize: 96, color: "red" } });
  await run("add", file, "--track", "BG", "type=image", "src=pic.png", "duration=40", "fit=contain");

  expect((await run("set", file, "text-1", "style.color=blue", "fadeIn=5")).clip).toMatchObject({ style: { fontSize: 96, color: "blue" }, fadeIn: 5 });
  expect((await run("set", file, "T1", "name=Titles", "muted=true")).track).toMatchObject({ name: "Titles", muted: true });
  expect((await run("set", file, "project", "duration=60")).duration).toBe(60);
  expect((await run("move", file, "text-1", "--start", "30")).clip.start).toBe(30);
  const split = await run("split", file, "video-1", "--at", "15");
  expect(split.clips.map((c: { id: string; trim: number }) => [c.id, c.trim])).toEqual([["video-1", 10], ["video-2", 25]]);
  expect((await run("remove", file, "image-1")).removed).toBe("image-1");

  const shown = await run("show", file);
  expect(shown.duration).toBe(60);
  expect(shown.project.tracks[1].clips.map((c: { id: string }) => c.id)).toEqual(["video-1", "video-2"]);
  expect((await run("validate", file)).problems).toEqual([]);
});

test("bad edits fail with a reason and leave the file alone", async () => {
  const before = readFileSync(file, "utf8");
  await expect(run("add", file, "--track", "V1", "type=text", "text=x", "duration=10", "start=0")).rejects.toThrow(/overlap/);
  await expect(run("add", file, "--track", "V1", "type=text", "text=x")).rejects.toThrow("add needs duration=<frames> for a text clip");
  await expect(run("add", file, "--track", "V1", "type=gif")).rejects.toThrow("add needs type=<");
  await expect(run("set", file, "nope", "x=1")).rejects.toThrow(/no clip "nope"/);
  await expect(run("move", file, "text-1", "--start", "1.5")).rejects.toThrow("--start must be a whole number (got 1.5)");
  await expect(run("split", file, "video-1")).rejects.toThrow("split needs --at <frame>");
  await expect(run("remove", file)).rejects.toThrow("remove needs a clip or track");
  await expect(run("show", join(dir, "missing.json"))).rejects.toThrow(/cannot read/);
  await expect(run("nope")).rejects.toThrow('no command "nope"');
  expect(readFileSync(file, "utf8")).toBe(before);
});

test("validate fails with every problem; show lists them too", async () => {
  const p = saved();
  p.tracks[1].clips[0].src = "gone.mp4";
  writeFileSync(file, JSON.stringify(p));
  const error = await run("validate", file).catch((e) => e);
  expect(error.problems).toEqual([{ path: 'tracks[1].clips[0] ("video-1").src', message: "file not found: gone.mp4" }]);
  expect((await run("show", file)).problems).toHaveLength(1);
  writeFileSync(join(dir, "broken.json"), "{");
  await expect(run("show", join(dir, "broken.json"))).rejects.toThrow("is not valid JSON");
});

test("probe, fields and help", async () => {
  expect(await run("probe", join(dir, "clip.mp4"), "--fps", "50")).toMatchObject({ seconds: 2, width: 320, height: 240, fps: 25, video: true, audio: true, image: false, frames: 100, at_fps: 50 });
  expect(await run("probe", join(dir, "pic.png"))).toMatchObject({ seconds: null, width: 64, image: true, video: false, audio: false });
  await expect(run("probe", join(dir, "missing.mp4"))).rejects.toThrow(/ffprobe failed/);
  expect(Object.keys((await run("fields")).clips)).toEqual(["video", "image", "audio", "text", "color", "component"]);
  expect((await run("fields", "text")).text.text).toMatchObject({ type: "string", multiline: true });
  await expect(run("fields", "gif")).rejects.toThrow('no clip type "gif"');
  expect((await run("help")).help).toContain("karkataka <command>");
});

test("the karkataka command prints JSON and exits 1 on failure", () => {
  const cli = join(import.meta.dirname, "cli.ts");
  const ok = JSON.parse(execFileSync("node", [cli, "fields", "color"], { encoding: "utf8" }));
  expect(ok).toMatchObject({ ok: true, result: { color: { color: { type: "string" } } } });
  let failure: { status: number; stdout: string } | undefined;
  try {
    execFileSync("node", [cli, "validate", file], { encoding: "utf8" });
  } catch (e) {
    failure = e as { status: number; stdout: string };
  }
  expect(failure!.status).toBe(1);
  expect(JSON.parse(failure!.stdout)).toMatchObject({ ok: false, error: expect.stringContaining("1 problem"), problems: [{ message: "file not found: gone.mp4" }] });
});
