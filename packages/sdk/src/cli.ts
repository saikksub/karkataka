#!/usr/bin/env node
// karkataka: edit, check, look at and render video projects from the command line. Every command
// prints one JSON object: {"ok": true, "result": ...} or {"ok": false, "error": "...", "problems"?: [...]}.

import { existsSync } from "node:fs";
import { parseArgs } from "node:util";
import { filesNextTo, loadProject, resolveIn, saveProject } from "./file.ts";
import { framesIn, probe } from "./media.ts";
import { addClip, addTrack, createProject, EditError, find, moveClip, remove, splitClip, updateClip, updateProject, updateTrack, type NewClip } from "./ops.ts";
import { CLIP_FIELDS, CLIP_TYPES, durationOf, PROJECT_FIELDS, validate, type ClipType, type Project } from "./project.ts";
import { ProjectError, renderProject, stillProject } from "./render.ts";

const HELP = `karkataka <command> ...   (every command prints JSON)

  new <project.json> [--width 1920] [--height 1080] [--fps 30] [--background #000] [--force]
  show <project.json>                       the project, its length and its problems
  validate <project.json>                   problems, if any (exit code 1 when there are)
  fields [type]                             the fields of each clip type and of the project
  probe <media> [--fps 30]                  length, size and streams of a media file

  add-track <project.json> [--name V2] [--id V2] [--index 0]     track 0 is the bottom layer
  add <project.json> --track <id> type=<type> key=value ...       e.g. type=video src=clip.mp4 trim=30
                                            no start: after the track's last clip
                                            no duration (video, audio): the rest of the file
  set <project.json> <clip|track|project> key=value ...           e.g. style.fontSize=96 fadeIn=null
  move <project.json> <clip> [--start <frame>] [--track <id>]
  split <project.json> <clip> --at <frame>
  remove <project.json> <clip|track>

  still <project.json> [--frame 0] [--out out/still.png] [--software]
  render <project.json> [--out out/video.mp4] [--software]   progress: JSON lines on stderr

Values are JSON when they parse as JSON (30, true, null, {"x":0}), else text. Dotted keys set
nested fields (box.x=100). Times are frames at the project's fps. Media paths are relative to the
project file. karka is $KARKA, else karka on PATH.`;

async function main(argv: string[]): Promise<unknown> {
  const [command, ...rest] = argv;
  const { values: o, positionals: args } = parseArgs({
    args: rest,
    allowPositionals: true,
    options: {
      width: { type: "string" }, height: { type: "string" }, fps: { type: "string" }, background: { type: "string" },
      force: { type: "boolean" }, name: { type: "string" }, id: { type: "string" }, index: { type: "string" },
      track: { type: "string" }, start: { type: "string" }, at: { type: "string" }, frame: { type: "string" },
      out: { type: "string" }, software: { type: "boolean" },
    },
  });
  const file = args[0];
  const need = (what: string, v: string | undefined) => {
    if (v === undefined) throw new Error(`${command} needs ${what} (see karkataka help)`);
    return v;
  };
  const int = (name: string, v: string) => {
    const n = Number(v);
    if (!Number.isInteger(n)) throw new Error(`--${name} must be a whole number (got ${v})`);
    return n;
  };
  const load = () => loadProject(need("a project file", file));
  const save = async (project: Project, result: Record<string, unknown>) => {
    await saveProject(file, project);
    return { ...result, duration: durationOf(project), problems: validate(project, filesNextTo(file)) };
  };

  switch (command) {
    case "new": {
      need("a project file", file);
      if (existsSync(file) && !o.force) throw new Error(`${file} exists; pass --force to replace it`);
      const settings: Record<string, unknown> = {};
      for (const key of ["width", "height", "fps"] as const) if (o[key] !== undefined) settings[key] = int(key, o[key]!);
      if (o.background !== undefined) settings.background = o.background;
      const project = createProject(settings);
      await saveProject(file, project);
      return { project };
    }
    case "show": {
      const project = await load();
      return { project, duration: durationOf(project), problems: validate(project, filesNextTo(file)) };
    }
    case "validate": {
      const problems = validate(await load(), filesNextTo(file));
      if (problems.length) throw new ProjectError(problems);
      return { problems };
    }
    case "fields": {
      const type = args[0] as ClipType | undefined;
      if (type !== undefined && !CLIP_TYPES.includes(type)) throw new Error(`no clip type "${type}" (types: ${CLIP_TYPES.join(", ")})`);
      return type ? { [type]: CLIP_FIELDS[type] } : { project: PROJECT_FIELDS, clips: CLIP_FIELDS, every_clip: { id: "unique; given or made", type: CLIP_TYPES.join(" | ") } };
    }
    case "probe": {
      const info = await probe(need("a media file", file));
      const fps = o.fps === undefined ? 30 : int("fps", o.fps);
      return { ...info, ...(info.seconds !== null && { frames: framesIn(info.seconds, fps), at_fps: fps }) };
    }
    case "add-track": {
      const { project, track } = addTrack(await load(), { name: o.name, id: o.id, index: o.index === undefined ? undefined : int("index", o.index) });
      return save(project, { track });
    }
    case "add": {
      const project = await load();
      const fields = assignments(args.slice(1));
      const type = fields.type as ClipType;
      if (!CLIP_TYPES.includes(type)) throw new Error(`add needs type=<${CLIP_TYPES.join("|")}>`);
      coerce(type, fields);
      if (fields.duration === undefined && (type === "video" || type === "audio") && typeof fields.src === "string") {
        const info = await probe(resolveIn(file, fields.src));
        if (info.seconds !== null) fields.duration = framesIn(info.seconds, project.fps) - (Number(fields.trim) || 0);
      }
      if (fields.duration === undefined) throw new Error(`add needs duration=<frames> for a ${type} clip`);
      const added = addClip(project, need("--track <id>", o.track), fields as NewClip);
      return save(added.project, { clip: added.clip });
    }
    case "set": {
      const project = await load();
      const id = need("a clip, track or project to change", args[1]);
      const fields = assignments(args.slice(2));
      if (id === "project") return save(updateProject(project, fields), {});
      if (project.tracks.some((t) => t.id === id)) {
        const { project: p, track } = updateTrack(project, id, fields);
        return save(p, { track });
      }
      coerce(find(project, id).clip.type, fields);
      const { project: p, clip } = updateClip(project, id, fields);
      return save(p, { clip });
    }
    case "move": {
      const { project, clip } = moveClip(await load(), need("a clip", args[1]), { start: o.start === undefined ? undefined : int("start", o.start), track: o.track });
      return save(project, { clip });
    }
    case "split": {
      const { project, clips } = splitClip(await load(), need("a clip", args[1]), int("at", need("--at <frame>", o.at)));
      return save(project, { clips });
    }
    case "remove": {
      const { project } = remove(await load(), need("a clip or track", args[1]));
      return save(project, { removed: args[1] });
    }
    case "still":
      return stillProject(need("a project file", file), o.frame === undefined ? 0 : int("frame", o.frame), o.out ?? "out/still.png", { software: o.software });
    case "render":
      return renderProject(need("a project file", file), o.out ?? "out/video.mp4", {
        software: o.software,
        onProgress: (frame, frames) => process.stderr.write(JSON.stringify({ frame, frames }) + "\n"),
      });
    case undefined:
    case "help":
    case "--help":
      return { help: HELP };
    default:
      throw new Error(`no command "${command}" (see karkataka help)`);
  }
}

/** `a=1 b.c="x"` as {a: 1, b: {c: "x"}}. Values are JSON when they parse, else text. */
export function assignments(pairs: string[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const pair of pairs) {
    const eq = pair.indexOf("=");
    if (eq <= 0) throw new Error(`expected key=value, got "${pair}"`);
    const keys = pair.slice(0, eq).split(".");
    const text = pair.slice(eq + 1);
    let value: unknown;
    try {
      value = JSON.parse(text);
    } catch {
      value = text;
    }
    let target = out;
    for (const key of keys.slice(0, -1)) target = (target[key] ??= {}) as Record<string, unknown>;
    target[keys.at(-1)!] = value;
  }
  return out;
}

/** Text fields keep what was typed: `text=2024` stays the text "2024", not the number. */
function coerce(type: ClipType, fields: Record<string, unknown>) {
  for (const [key, schema] of Object.entries(CLIP_FIELDS[type])) {
    if (schema.type === "string" && fields[key] !== undefined && fields[key] !== null && typeof fields[key] !== "string") fields[key] = JSON.stringify(fields[key]);
  }
}

if (import.meta.main) {
  main(process.argv.slice(2)).then(
    (result) => console.log(JSON.stringify(result && "help" in (result as object) ? result : { ok: true, result }, null, 2)),
    (e: Error) => {
      console.log(JSON.stringify({ ok: false, error: e.message, ...(e instanceof ProjectError || e instanceof EditError ? { problems: e.problems } : {}) }, null, 2));
      process.exitCode = 1;
    },
  );
}

export { main };
