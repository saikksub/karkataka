// Media files: what they contain (ffprobe) and video frames as images (ffmpeg).

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readdir, rm, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { Fit } from "./project.ts";

export interface MediaInfo {
  /** Length in seconds; `null` for still images. */
  seconds: number | null;
  /** Size of the picture, if the file has one. */
  width?: number;
  height?: number;
  /** The file's own frame rate, for video. */
  fps?: number;
  video: boolean;
  audio: boolean;
  image: boolean;
}

/** What `path` contains. Fails with ffprobe's message when it cannot read it. */
export async function probe(path: string): Promise<MediaInfo> {
  const out = await run("ffprobe", ["-v", "error", "-show_entries", "format=duration,format_name:stream=codec_type,width,height,avg_frame_rate", "-of", "json", path]);
  const info = JSON.parse(out) as { format: { duration?: string; format_name: string }; streams: { codec_type: string; width?: number; height?: number; avg_frame_rate?: string }[] };
  const picture = info.streams.find((s) => s.codec_type === "video");
  // Still images (PNG, JPEG, WebP...) come through ffmpeg's image readers; an animated GIF counts as video.
  const image = /image2|_pipe/.test(info.format.format_name);
  const [num, den] = (picture?.avg_frame_rate ?? "0/1").split("/").map(Number);
  return {
    seconds: image || info.format.duration === undefined ? null : Number(info.format.duration),
    ...(picture && { width: picture.width, height: picture.height }),
    ...(!image && picture && den > 0 && num > 0 && { fps: Math.round((num / den) * 1000) / 1000 }),
    video: Boolean(picture) && !image,
    audio: info.streams.some((s) => s.codec_type === "audio"),
    image,
  };
}

/** Whole frames of `seconds` at `fps`. */
export const framesIn = (seconds: number, fps: number) => Math.floor(seconds * fps + 1e-6);

export interface FrameRange {
  /** Source frames skipped before the clip starts (the clip's trim). */
  trim: number;
  /** First and count of the clip's own frames to extract. */
  first: number;
  count: number;
  fps: number;
  /** The box the frames fill, and how. Frames are scaled to it, so they are no bigger than needed. */
  width: number;
  height: number;
  fit: Fit;
}

/**
 * Extract frames of the video `src` as JPEGs `000001.jpg`... into a folder under `cacheDir`
 * named after the file and range, and return it with how many frames the source had. A folder
 * already done is reused.
 */
export async function extractFrames(src: string, range: FrameRange, cacheDir: string): Promise<{ dir: string; count: number }> {
  // A URL has no file to stat; its cached frames are kept until the cache is cleared.
  const { mtimeMs, size } = src.includes("://") ? { mtimeMs: 0, size: 0 } : await stat(src);
  const key = createHash("sha1").update(JSON.stringify({ src, mtimeMs, size, ...range })).digest("hex").slice(0, 16);
  const dir = join(cacheDir, key);
  const done = join(dir, ".done");
  if (!existsSync(done)) {
    await rm(dir, { recursive: true, force: true });
    await mkdir(dir, { recursive: true });
    const scale = {
      cover: `scale=${range.width}:${range.height}:force_original_aspect_ratio=increase`,
      contain: `scale=${range.width}:${range.height}:force_original_aspect_ratio=decrease`,
      fill: `scale=${range.width}:${range.height}`,
    }[range.fit];
    const at = (range.trim + range.first) / range.fps;
    await run("ffmpeg", ["-v", "error", "-ss", String(at), "-i", src, "-frames:v", String(range.count), "-vf", `fps=${range.fps},${scale}`, "-q:v", "3", join(dir, "%06d.jpg")]);
    await writeFile(done, "");
  }
  // The source may end before the range does.
  const count = (await readdir(dir)).filter((f) => f.endsWith(".jpg")).length;
  return { dir, count };
}

/** Run a command; resolve with its stdout, or fail with its stderr. */
export function run(command: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (d) => (stdout += d));
    child.stderr.on("data", (d) => (stderr += d));
    child.on("error", (e) => reject(new Error(`cannot run ${command} (${e.message}); install FFmpeg and put it on PATH`)));
    child.on("close", (code) => (code === 0 ? resolve(stdout) : reject(new Error(`${command} failed: ${stderr.trim() || `exit code ${code}`}`))));
  });
}
