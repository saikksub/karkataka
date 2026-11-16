// Edits to a project. Each returns a new project and leaves the old one alone (handy for undo),
// and refuses an edit that would break the project, saying why.

import { CLIP_FIELDS, durationOf, validate, VERSION, type Clip, type ClipType, type Problem, type Project, type Track } from "./project.ts";

export class EditError extends Error {
  problems: Problem[];
  constructor(message: string, problems: Problem[] = []) {
    super(message);
    this.problems = problems;
  }
}

/** A clip as given to `addClip`: no id needed, and no start to place it after the track's last clip. */
export type NewClip = { [T in ClipType]: Omit<Extract<Clip, { type: T }>, "id" | "start"> & { id?: string; start?: number } }[ClipType];

export function createProject(settings: { width?: number; height?: number; fps?: number; duration?: number; background?: string } = {}): Project {
  const project: Project = { version: VERSION, width: 1920, height: 1080, fps: 30, ...settings, tracks: [] };
  check(project, []);
  return project;
}

/** Change size, fps, duration or background. `null` removes duration or background. */
export function updateProject(project: Project, patch: Record<string, unknown>): Project {
  return edit(project, (p) => {
    for (const [key, value] of Object.entries(patch)) {
      if (!["width", "height", "fps", "duration", "background"].includes(key)) throw new EditError(`"${key}" is not a project setting (width, height, fps, duration, background)`);
      if (value === null) delete (p as unknown as Record<string, unknown>)[key];
      else (p as unknown as Record<string, unknown>)[key] = value;
    }
  }).project;
}

export function addTrack(project: Project, options: { name?: string; id?: string; index?: number } = {}): { project: Project; track: Track } {
  return edit(project, (p) => {
    const track: Track = { id: options.id ?? newId(p, "track"), clips: [] };
    if (options.name) track.name = options.name;
    p.tracks.splice(options.index ?? p.tracks.length, 0, track);
    return track;
  }, "track");
}

/** Change a track's name, hidden or muted. */
export function updateTrack(project: Project, id: string, patch: { name?: string; hidden?: boolean; muted?: boolean }): { project: Project; track: Track } {
  return edit(project, (p) => {
    const track = findTrack(p, id);
    for (const [key, value] of Object.entries(patch)) {
      if (!["name", "hidden", "muted"].includes(key)) throw new EditError(`"${key}" is not a track setting (name, hidden, muted)`);
      if (value === null) delete (track as unknown as Record<string, unknown>)[key];
      else (track as unknown as Record<string, unknown>)[key] = value;
    }
    return track;
  }, "track");
}

export function addClip(project: Project, trackId: string, clip: NewClip): { project: Project; clip: Clip } {
  return edit(project, (p) => {
    const track = findTrack(p, trackId);
    const start = clip.start ?? Math.max(0, ...track.clips.map((c) => c.start + c.duration));
    const added = { ...clip, id: clip.id ?? newId(p, clip.type), start } as Clip;
    track.clips.push(added);
    track.clips.sort((a, b) => a.start - b.start);
    return added;
  }, "clip");
}

/**
 * Change a clip's fields. Objects (`box`, `style`, `props`) are merged one level deep, so
 * `{style: {color: "red"}}` keeps the other style fields. `null` removes a field.
 */
export function updateClip(project: Project, id: string, patch: Record<string, unknown>): { project: Project; clip: Clip } {
  return edit(project, (p) => {
    const { track, clip } = find(p, id);
    if ("id" in patch || "type" in patch) throw new EditError("a clip's id and type cannot change; remove it and add a new one");
    const fields = clip as unknown as Record<string, unknown>;
    for (const [key, value] of Object.entries(patch)) {
      if (!(key in CLIP_FIELDS[clip.type])) throw new EditError(`"${key}" is not a ${clip.type} clip field (fields: ${Object.keys(CLIP_FIELDS[clip.type]).join(", ")})`);
      const current = fields[key];
      if (value === null) delete fields[key];
      else if (isObject(value) && isObject(current)) fields[key] = withoutNulls({ ...current, ...value });
      else fields[key] = value;
    }
    track.clips.sort((a, b) => a.start - b.start);
    return clip;
  }, "clip");
}

/** Move a clip to another start frame, another track, or both. */
export function moveClip(project: Project, id: string, to: { start?: number; track?: string }): { project: Project; clip: Clip } {
  return edit(project, (p) => {
    const { track, clip } = find(p, id);
    if (to.start !== undefined) clip.start = to.start;
    if (to.track !== undefined && to.track !== track.id) {
      const target = findTrack(p, to.track);
      track.clips.splice(track.clips.indexOf(clip), 1);
      target.clips.push(clip);
    }
    for (const t of p.tracks) t.clips.sort((a, b) => a.start - b.start);
    return clip;
  }, "clip");
}

/**
 * Cut a clip in two at video frame `at`. The second part keeps playing the source where the
 * first stopped (its trim moves on); fades stay at the outer ends.
 */
export function splitClip(project: Project, id: string, at: number): { project: Project; clips: [Clip, Clip] } {
  return edit(project, (p) => {
    const { track, clip } = find(p, id);
    if (!Number.isInteger(at) || at <= clip.start || at >= clip.start + clip.duration) {
      throw new EditError(`split frame must be inside the clip, from ${clip.start + 1} to ${clip.start + clip.duration - 1} (got ${at})`);
    }
    const first = clip.duration - (clip.start + clip.duration - at);
    const second = { ...structuredClone(clip), id: newId(p, clip.type), start: at, duration: clip.duration - first } as Clip;
    clip.duration = first;
    delete clip.fadeOut;
    delete second.fadeIn;
    if (second.type === "video" || second.type === "audio") second.trim = (second.trim ?? 0) + first;
    track.clips.splice(track.clips.indexOf(clip) + 1, 0, second);
    return [clip, second] as [Clip, Clip];
  }, "clips");
}

/** Remove a clip or a track (with its clips). */
export function remove(project: Project, id: string): { project: Project } {
  return edit(project, (p) => {
    const t = p.tracks.findIndex((track) => track.id === id);
    if (t >= 0) return void p.tracks.splice(t, 1);
    const { track, clip } = find(p, id);
    track.clips.splice(track.clips.indexOf(clip), 1);
  });
}

/** The clip with `id` and its track. */
export function find(project: Project, id: string): { track: Track; clip: Clip } {
  for (const track of project.tracks) {
    const clip = track.clips.find((c) => c.id === id);
    if (clip) return { track, clip };
  }
  throw new EditError(`no clip "${id}" (clips: ${project.tracks.flatMap((t) => t.clips.map((c) => c.id)).join(", ") || "none"})`);
}

function findTrack(project: Project, id: string): Track {
  const track = project.tracks.find((t) => t.id === id);
  if (!track) throw new EditError(`no track "${id}" (tracks: ${project.tracks.map((t) => t.id).join(", ") || "none"})`);
  return track;
}

/** `type-N` with the first N not taken by a track or clip. */
function newId(project: Project, type: string): string {
  const taken = new Set(project.tracks.flatMap((t) => [t.id, ...t.clips.map((c) => c.id)]));
  let n = 1;
  while (taken.has(`${type}-${n}`)) n++;
  return `${type}-${n}`;
}

/** Run `change` on a copy; refuse it if it adds problems (problems already there are not its fault). */
function edit<T, K extends string = "result">(project: Project, change: (draft: Project) => T, key?: K): { project: Project } & Record<K, T> {
  const draft = structuredClone(project);
  const result = change(draft);
  check(draft, validate(project));
  return { project: draft, [key ?? "result"]: result } as { project: Project } & Record<K, T>;
}

function check(project: Project, before: Problem[]) {
  const known = new Set(before.map((p) => `${p.path}: ${p.message}`));
  const added = validate(project).filter((p) => !known.has(`${p.path}: ${p.message}`));
  if (added.length) throw new EditError(added.map((p) => `${p.path} ${p.message}`).join("; "), added);
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const withoutNulls = (o: Record<string, unknown>) => Object.fromEntries(Object.entries(o).filter(([, v]) => v !== null));

export { durationOf };
