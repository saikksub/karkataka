// The project file's shape, for the player and the Studio. The rules (validation, edits) live in
// the karka binary.

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

