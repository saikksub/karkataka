// Rendering a project with karka: check it, prepare its media, build the player, run karka.

import { spawn } from "node:child_process";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { buildPlayer } from "./build.ts";
import { filesNextTo, loadProject, resolveIn } from "./file.ts";
import { extractFrames, probe, type MediaInfo } from "./media.ts";
import type { PlayerClip, PlayerProject } from "./player/types.ts";
import { durationOf, validate, type Problem, type Project } from "./project.ts";

/** The project has problems; `problems` lists them all. */
export class ProjectError extends Error {
  problems: Problem[];
  constructor(problems: Problem[]) {
    super(`the project has ${problems.length} problem${problems.length === 1 ? "" : "s"}:\n${problems.map((p) => `  ${p.path} ${p.message}`).join("\n")}`);
    this.problems = problems;
  }
}

export interface EngineOptions {
  /** The karka executable. Default: `$KARKA`, else `karka` on PATH. */
  karka?: string;
  /** Paint on the CPU (often faster; GPU is faster for 3D and filters). */
  software?: boolean;
  /** Stop: karka is killed. */
  signal?: AbortSignal;
}

export interface RenderOptions extends EngineOptions {
  /** Called as each frame is done. */
  onProgress?: (frame: number, frames: number) => void;
}

/** Render the project at `projectPath` to an MP4 at `out`. Resolves with karka's result. */
export async function renderProject(projectPath: string, out: string, options: RenderOptions = {}): Promise<Record<string, unknown>> {
  const prepared = await prepare(projectPath, (duration) => [0, duration]);
  try {
    const { page, project } = prepared;
    const args = ["render", page, ...size(project), "--out", resolve(out), "--json", "--progress", ...(options.software ? ["--software"] : [])];
    return await karka(args, options, (frame) => options.onProgress?.(frame, project.duration));
  } finally {
    await prepared.cleanup();
  }
}

/** Frame `frame` of the project as a PNG at `out`: how an agent looks at its work. */
export async function stillProject(projectPath: string, frame: number, out: string, options: EngineOptions = {}): Promise<Record<string, unknown>> {
  const prepared = await prepare(projectPath, (duration) => {
    if (!Number.isInteger(frame) || frame < 0 || frame >= duration) throw new Error(`frame must be from 0 to ${duration - 1} (got ${frame})`);
    return [frame, frame + 1];
  });
  try {
    const { page, project } = prepared;
    const args = ["still", page, ...size(project), "--frame", String(frame), "--out", resolve(out), "--json", ...(options.software ? ["--software"] : [])];
    return await karka(args, options);
  } finally {
    await prepared.cleanup();
  }
}

const size = (p: PlayerProject) => ["--frames", String(p.duration), "--width", String(p.width), "--height", String(p.height), "--fps", String(p.fps)];

/** Check the project, extract the video frames in `range` (from the duration), and build its page. */
async function prepare(projectPath: string, range: (duration: number) => [number, number]) {
  const project = await loadProject(projectPath);
  const problems = validate(project, filesNextTo(projectPath));
  if (problems.length) throw new ProjectError(problems);
  const [from, to] = range(durationOf(project));
  const work = join(dirname(resolve(projectPath)), ".karka");
  await mkdir(work, { recursive: true });
  const annotated = await annotate(projectPath, project, { url: (path) => pathToFileURL(path).href, frames: { cacheDir: join(work, "frames"), from, to } });
  const dist = await mkdtemp(join(work, "build-"));
  return { project: annotated, page: await buildPlayer(projectPath, annotated, dist), cleanup: () => rm(dist, { recursive: true, force: true }) };
}

/**
 * The project with what the player needs to load its media: each file's URL (`url` maps an
 * absolute path to one) and path, and whether video has sound. With `frames`, the video frames in
 * `from..to` are extracted to images; without, the player seeks a <video> (the Studio preview).
 */
export async function annotate(
  projectPath: string,
  project: Project,
  options: { url: (path: string) => string; frames?: { cacheDir: string; from: number; to: number } },
): Promise<PlayerProject> {
  const probes = new Map<string, Promise<MediaInfo>>();
  const probed = (path: string) => probes.get(path) ?? probes.set(path, probe(path)).get(path)!;
  const clip = async (c: Project["tracks"][number]["clips"][number]): Promise<PlayerClip> => {
    if (c.type !== "video" && c.type !== "image" && c.type !== "audio") return c;
    const path = c.src.includes("://") ? c.src : resolveIn(projectPath, c.src);
    const out: PlayerClip = { ...c, path, url: c.src.includes("://") ? c.src : options.url(path) };
    if (c.type !== "video") return out;
    out.audio = (await probed(path)).audio;
    if (options.frames) {
      const { cacheDir, from, to } = options.frames;
      const first = Math.max(from - c.start, 0);
      const count = Math.min(to - c.start, c.duration) - first;
      const box = c.box ?? { width: project.width, height: project.height };
      if (count <= 0) out.frames = { url: "", first: 0, count: 0 };
      else {
        const range = { trim: c.trim ?? 0, first, count, fps: project.fps, width: even(box.width), height: even(box.height), fit: c.fit ?? "cover" };
        const extracted = await extractFrames(path, range, cacheDir);
        out.frames = { url: options.url(extracted.dir) + "/", first, count: extracted.count };
      }
    }
    return out;
  };
  const tracks = await Promise.all(project.tracks.map(async (t) => ({ ...t, clips: await Promise.all(t.clips.map(clip)) })));
  return { ...project, duration: durationOf(project), base: dirname(resolve(projectPath)) + "/", tracks };
}

const even = (n: number) => Math.max(2, Math.round(n / 2) * 2);

/** Run karka; report `--progress` lines; resolve with its `--json` result, or fail with its error. */
export function karka(args: string[], options: EngineOptions, onFrame?: (frame: number) => void): Promise<Record<string, unknown>> {
  const command = options.karka ?? process.env.KARKA ?? "karka";
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { stdio: ["ignore", "pipe", "pipe"], signal: options.signal });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (d) => (stdout += d));
    createInterface({ input: child.stderr }).on("line", (line) => {
      const frame = progressFrame(line);
      if (frame === undefined) stderr += line + "\n";
      else onFrame?.(frame);
    });
    child.on("error", (e) =>
      reject(e.name === "AbortError" ? new Error("stopped") : new Error(`cannot run karka (${command}): ${e.message}. Set KARKA or pass the karka option.`)),
    );
    child.on("close", () => {
      try {
        const out = JSON.parse(stdout.trim().split("\n").pop()!);
        if (out.ok) resolvePromise(out.result);
        else reject(new Error(out.error));
      } catch {
        reject(new Error(stderr.trim() || "karka exited without a result"));
      }
    });
  });
}

/** The frame count in a `karka render --progress` line, if `line` is one. */
export function progressFrame(line: string): number | undefined {
  try {
    const p = JSON.parse(line);
    return typeof p?.frame === "number" ? p.frame : undefined;
  } catch {
    return undefined;
  }
}
