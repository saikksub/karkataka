// The Studio's side of `karka studio`: its HTTP API, and the little project arithmetic the UI
// needs. The rules (validation, edits) are the karka binary's; the UI only asks for edits.

import type { PlayerProject } from "@karkataka/player";
import type { Clip, Project } from "@karkataka/player/project";
import type { PropSchema } from "@karkataka/runtime";

export interface Problem {
  path: string;
  message: string;
}

/** What GET /api/project returns. */
export interface State {
  project: Project;
  file: string;
  problems: Problem[];
}

export type Fields = Record<string, PropSchema>;

/** An edit, as `karka add`, `set`, `move`, `split`, `remove` and `add-track` make it. */
export type Edit =
  | { op: "add"; track: string; fields: Record<string, unknown> }
  | { op: "set"; target: string; fields: Record<string, unknown> }
  | { op: "move"; clip: string; start?: number; track?: string }
  | { op: "split"; clip: string; at: number }
  | { op: "remove"; id: string }
  | { op: "add-track"; id?: string; name?: string; index?: number };

/** A message on the render stream (one JSON object per line). */
export type RenderEvent =
  | { type: "building" }
  | { type: "progress"; frame: number; frames: number }
  | { type: "done"; url: string; result: unknown }
  | { type: "error"; message: string };

/** The server refused: why, and the problems it found. */
export class ApiError extends Error {
  problems: Problem[];
  constructor(message: string, problems: Problem[] = []) {
    super(message);
    this.problems = problems;
  }
}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, { method, body: body === undefined ? undefined : JSON.stringify(body) });
  const json = await res.json();
  if (!res.ok) throw new ApiError(json.error ?? res.statusText, json.problems ?? []);
  return json as T;
}

export const api = {
  project: () => call<State>("GET", "/api/project"),
  /** Replace the whole project (undo and redo). */
  replace: (project: Project) => call<State>("PUT", "/api/project", project),
  edit: (edit: Edit) => call<State & { result: Record<string, unknown> }>("POST", "/api/edit", edit),
  fields: () => call<{ project: Fields; clips: Record<string, Fields> }>("GET", "/api/fields"),
};

/** Length in frames: `duration`, or where the last clip ends. */
export function durationOf(project: Project): number {
  return project.duration ?? Math.max(1, ...project.tracks.flatMap((t) => t.clips.map((c) => c.start + c.duration)));
}

export function findClip(project: Project, id: string | undefined): Clip | undefined {
  return project.tracks.flatMap((t) => t.clips).find((c) => c.id === id);
}

const path = (p: string) => p.replace(/^\.\//, "");

/** The project as the player draws it here: media from /files/, scenes by their import keys. */
export function forPlayer(project: Project): PlayerProject {
  const media = (src: string) => (src.includes("://") ? src : `/files/${path(src).split("/").map(encodeURIComponent).join("/")}`);
  const clip = (c: Clip) =>
    c.type === "component" ? { ...c, module: `@project/${path(c.component)}` } : c.type === "video" || c.type === "image" || c.type === "audio" ? { ...c, url: media(c.src) } : c;
  return { ...project, duration: durationOf(project), tracks: project.tracks.map((t) => ({ ...t, clips: t.clips.map(clip) })) };
}

/** The scene files the project uses: the player page compiles only these, so a new one needs a new page. */
export function scenesOf(project: Project): string {
  return [...new Set(project.tracks.flatMap((t) => t.clips.flatMap((c) => (c.type === "component" ? [path(c.component)] : []))))].sort().join(",");
}
