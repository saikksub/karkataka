// The project file: what a video is made of. Plain JSON, so agents and people can read and edit it.
// Times are in frames of the project's fps. Track 0 is the bottom layer; later tracks draw over it.

import type { PropSchema } from "@karkataka/runtime";

export const VERSION = 1;

export interface Project {
  version: 1;
  width: number;
  height: number;
  fps: number;
  /** Length in frames. Default: where the last clip ends. */
  duration?: number;
  /** CSS color behind every track. Default black. */
  background?: string;
  tracks: Track[];
}

export interface Track {
  id: string;
  name?: string;
  /** Hidden tracks draw nothing. */
  hidden?: boolean;
  /** Muted tracks play no sound. */
  muted?: boolean;
  clips: Clip[];
}

/** Where a visual clip sits, in px. Default: the whole frame. */
export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type Fit = "cover" | "contain" | "fill";

interface ClipBase {
  id: string;
  /** First frame in the video. */
  start: number;
  /** Length in frames. */
  duration: number;
  box?: Box;
  /** 0 to 1. Default 1. */
  opacity?: number;
  /** Frames to fade in from transparent at the start, and out at the end. */
  fadeIn?: number;
  fadeOut?: number;
}

export interface TextStyle {
  fontSize?: number;
  fontFamily?: string;
  fontWeight?: number;
  color?: string;
  background?: string;
  align?: "left" | "center" | "right";
  verticalAlign?: "top" | "center" | "bottom";
  padding?: number;
  lineHeight?: number;
}

export type VideoClip = ClipBase & { type: "video"; src: string; trim?: number; volume?: number; fit?: Fit };
export type ImageClip = ClipBase & { type: "image"; src: string; fit?: Fit };
export type AudioClip = ClipBase & { type: "audio"; src: string; trim?: number; volume?: number };
export type TextClip = ClipBase & { type: "text"; text: string; style?: TextStyle };
export type ColorClip = ClipBase & { type: "color"; color: string };
/** A Solid component: `component` is a module path (relative to the project file) whose default export takes `props`. */
export type ComponentClip = ClipBase & { type: "component"; component: string; props?: Record<string, unknown> };

export type Clip = VideoClip | ImageClip | AudioClip | TextClip | ColorClip | ComponentClip;
export type ClipType = Clip["type"];
export const CLIP_TYPES: readonly ClipType[] = ["video", "image", "audio", "text", "color", "component"];

/** Fields every clip has. */
const common: Record<string, PropSchema> = {
  start: { type: "integer", minimum: 0, title: "Start (frame)" },
  duration: { type: "integer", minimum: 1, title: "Duration (frames)" },
};
const visual: Record<string, PropSchema> = {
  opacity: { type: "number", minimum: 0, maximum: 1, title: "Opacity" },
  fadeIn: { type: "integer", minimum: 0, title: "Fade in (frames)" },
  fadeOut: { type: "integer", minimum: 0, title: "Fade out (frames)" },
  box: { type: "object", title: "Box", description: "{x, y, width, height} in px. Default: the whole frame." },
};
const fit: PropSchema = { type: "string", enum: ["cover", "contain", "fill"], title: "Fit", description: "Default cover." };
const media = (kind: string): PropSchema => ({ type: "string", title: "Source", description: `${kind} file, relative to the project file.` });
const trim: PropSchema = { type: "integer", minimum: 0, title: "Trim (frames)", description: "Frames skipped at the start of the source." };
const volume: PropSchema = { type: "number", minimum: 0, title: "Volume", description: "1 is unchanged." };

/** The fields of each clip type, for docs, the CLI and the Studio's inspector. */
export const CLIP_FIELDS: Record<ClipType, Record<string, PropSchema>> = {
  video: { src: media("Video"), ...common, trim, volume, fit, ...visual },
  image: { src: media("Image"), ...common, fit, ...visual },
  audio: { src: media("Audio"), ...common, trim, volume },
  text: {
    text: { type: "string", multiline: true, title: "Text" },
    ...common,
    style: {
      type: "object",
      title: "Style",
      description: "fontSize, fontFamily, fontWeight, color, background, align (left|center|right), verticalAlign (top|center|bottom), padding, lineHeight.",
    },
    ...visual,
  },
  color: { color: { type: "string", title: "Color", description: "Any CSS color or gradient." }, ...common, ...visual },
  component: {
    component: { type: "string", title: "Component", description: "Module path relative to the project file; its default export is a Solid component." },
    ...common,
    props: { type: "object", title: "Props" },
    ...visual,
  },
};

export const PROJECT_FIELDS: Record<string, PropSchema> = {
  width: { type: "integer", minimum: 72, title: "Width (px)", description: "Even, at least 72." },
  height: { type: "integer", minimum: 2, title: "Height (px)", description: "Even." },
  fps: { type: "integer", minimum: 1, maximum: 240, title: "Frames per second" },
  duration: { type: "integer", minimum: 1, title: "Duration (frames)", description: "Default: where the last clip ends." },
  background: { type: "string", title: "Background", description: "CSS color. Default black." },
};

/** Length of the video in frames. */
export function durationOf(project: Project): number {
  if (project.duration !== undefined) return project.duration;
  return Math.max(1, ...project.tracks.flatMap((t) => t.clips.map((c) => c.start + c.duration)));
}

export interface Problem {
  /** Where, e.g. `tracks[0].clips[2].src` (and the clip's id when there is one). */
  path: string;
  message: string;
}

const isInt = (v: unknown, min: number) => Number.isInteger(v) && (v as number) >= min;
const isNum = (v: unknown, min: number, max = Infinity) => typeof v === "number" && Number.isFinite(v) && v >= min && v <= max;

/**
 * Everything wrong with `project`, or nothing. `fileExists` (given a `src` or `component` path as
 * written) also checks that media and component files exist.
 */
export function validate(project: unknown, fileExists?: (path: string) => boolean): Problem[] {
  const problems: Problem[] = [];
  const bad = (path: string, message: string) => problems.push({ path, message });
  const p = project as Project;
  if (typeof p !== "object" || p === null || Array.isArray(p)) return [{ path: "", message: "a project is a JSON object" }];
  if (p.version !== VERSION) bad("version", `must be ${VERSION}`);
  if (!isInt(p.width, 72) || p.width % 2) bad("width", "must be an even whole number, at least 72");
  if (!isInt(p.height, 2) || p.height % 2) bad("height", "must be an even whole number, at least 2");
  if (!isInt(p.fps, 1) || p.fps > 240) bad("fps", "must be a whole number from 1 to 240");
  if (p.duration !== undefined && !isInt(p.duration, 1)) bad("duration", "must be a whole number, at least 1");
  if (p.background !== undefined && typeof p.background !== "string") bad("background", "must be a CSS color");
  if (!Array.isArray(p.tracks)) {
    bad("tracks", "must be a list of tracks");
    return problems;
  }
  const ids = new Set<string>();
  const checkId = (path: string, id: unknown) => {
    if (typeof id !== "string" || id === "") bad(`${path}.id`, "must be a non-empty string");
    else if (ids.has(id)) bad(`${path}.id`, `"${id}" is used twice; ids must be unique`);
    else ids.add(id);
  };
  p.tracks.forEach((track, t) => {
    const tp = `tracks[${t}]`;
    if (typeof track !== "object" || track === null) return bad(tp, "must be an object");
    checkId(tp, track.id);
    if (!Array.isArray(track.clips)) return bad(`${tp}.clips`, "must be a list of clips");
    track.clips.forEach((clip, c) => checkClip(clip, `${tp}.clips[${c}]`, bad, checkId, fileExists));
    // Clips on one track must not overlap, as in any editor: put overlapping clips on another track.
    const sorted = track.clips.filter((c) => isInt(c?.start, 0) && isInt(c?.duration, 1)).sort((a, b) => a.start - b.start);
    for (let i = 1; i < sorted.length; i++) {
      const [a, b] = [sorted[i - 1], sorted[i]];
      if (b.start < a.start + a.duration) bad(`${tp}`, `clips "${a.id}" (frames ${a.start}–${a.start + a.duration - 1}) and "${b.id}" (from ${b.start}) overlap; move one or put it on another track`);
    }
  });
  return problems;
}

function checkClip(
  clip: Clip,
  path: string,
  bad: (path: string, message: string) => void,
  checkId: (path: string, id: unknown) => void,
  fileExists?: (path: string) => boolean,
) {
  if (typeof clip !== "object" || clip === null) return bad(path, "must be an object");
  checkId(path, clip.id);
  const at = `${path} ("${clip.id}")`;
  if (!CLIP_TYPES.includes(clip.type)) return bad(`${at}.type`, `must be one of ${CLIP_TYPES.join(", ")}`);
  if (!isInt(clip.start, 0)) bad(`${at}.start`, "must be a whole number of frames, at least 0");
  if (!isInt(clip.duration, 1)) bad(`${at}.duration`, "must be a whole number of frames, at least 1");
  if (clip.opacity !== undefined && !isNum(clip.opacity, 0, 1)) bad(`${at}.opacity`, "must be from 0 to 1");
  for (const fade of ["fadeIn", "fadeOut"] as const) {
    const v = clip[fade];
    if (v !== undefined && (!isInt(v, 0) || v > clip.duration)) bad(`${at}.${fade}`, "must be a whole number of frames, at most the clip's duration");
  }
  if (clip.box !== undefined) {
    const b = clip.box;
    if (typeof b !== "object" || b === null || !isNum(b.x, -Infinity) || !isNum(b.y, -Infinity) || !isNum(b.width, 0) || !isNum(b.height, 0)) {
      bad(`${at}.box`, "must be {x, y, width, height} in px");
    }
  }
  const allowed = new Set(["id", "type", ...Object.keys(CLIP_FIELDS[clip.type])]);
  for (const key of Object.keys(clip)) if (!allowed.has(key)) bad(`${at}.${key}`, `is not a ${clip.type} clip field (fields: ${[...allowed].join(", ")})`);
  const file = (key: "src" | "component", kind: string) => {
    const v = (clip as unknown as Record<string, unknown>)[key];
    if (typeof v !== "string" || v === "") bad(`${at}.${key}`, `must be the path of a ${kind} file`);
    else if (fileExists && !v.includes("://") && !fileExists(v)) bad(`${at}.${key}`, `file not found: ${v}`);
  };
  switch (clip.type) {
    case "video":
    case "audio":
      file("src", clip.type);
      if (clip.trim !== undefined && !isInt(clip.trim, 0)) bad(`${at}.trim`, "must be a whole number of frames, at least 0");
      if (clip.volume !== undefined && !isNum(clip.volume, 0)) bad(`${at}.volume`, "must be 0 or more");
      break;
    case "image":
      file("src", "image");
      break;
    case "text":
      if (typeof clip.text !== "string") bad(`${at}.text`, "must be a string");
      if (clip.style !== undefined && (typeof clip.style !== "object" || clip.style === null)) bad(`${at}.style`, "must be an object");
      break;
    case "color":
      if (typeof clip.color !== "string" || clip.color === "") bad(`${at}.color`, "must be a CSS color");
      break;
    case "component":
      file("component", "component");
      if (clip.props !== undefined && (typeof clip.props !== "object" || clip.props === null || Array.isArray(clip.props))) bad(`${at}.props`, "must be an object");
      break;
  }
  if ((clip.type === "video" || clip.type === "image") && clip.fit !== undefined && !["cover", "contain", "fill"].includes(clip.fit)) {
    bad(`${at}.fit`, "must be cover, contain or fill");
  }
}
