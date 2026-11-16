import { componentsOf, loadProject, PLAYER_DIR, playerModules, renderProject, saveProject, validate, filesNextTo, type Project } from "@karkataka/sdk";
import type { IncomingMessage, ServerResponse } from "node:http";
import { basename, dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { normalizePath, type Plugin, type ViteDevServer } from "vite";

export interface StudioOptions {
  /** The project file, relative to the Vite root. */
  project: string;
  /** The karka executable. Default: `$KARKA`, else `karka` on PATH. */
  karka?: string;
}

/** A message on the render stream (one JSON object per line). */
export type RenderEvent =
  | { type: "building" }
  | { type: "progress"; frame: number; frames: number }
  | { type: "done"; url: string; result: unknown }
  | { type: "error"; message: string };

/** What GET /__karka/project returns. */
export interface ProjectState {
  project: Project;
  /** The project's folder: media URLs are `/@fs` + this + the path written in the project. */
  dir: string;
  file: string;
  problems: ReturnType<typeof validate>;
}

// The Studio app is compiled by this dev server (the project's config has vite-plugin-solid).
// shortcut: works while the studio package is a workspace source package; a published one ships built JS.
const app = fsUrl(fileURLToPath(new URL("./app/main.tsx", import.meta.url)));
const player = fsUrl(join(PLAYER_DIR, "main.tsx"));

function fsUrl(path: string) {
  return `/@fs/${normalizePath(path).replace(/^\//, "")}`;
}

const page = (title: string, script: string, body: string) => `<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>${title}</title><link rel="icon" href="data:,"></head>
<body>${body}<script type="module" src="${script}"></script></body></html>`;

/**
 * The Studio for a project, in this Vite dev server: /studio is the editor, /player draws the
 * project. Edits are saved to the project file; changes to the file from elsewhere (an agent
 * using the SDK) show up at once.
 */
export function studio(options: StudioOptions): Plugin[] {
  let file = "";
  let components: Record<string, string> = {};
  const readComponents = async () => {
    try {
      components = componentsOf(file, await loadProject(file));
    } catch {
      components = {};
    }
  };
  return [
    playerModules(() => ({ components, project: null })),
    {
      name: "karkataka-studio",
      apply: "serve",
      // Components import solid-js and the runtime from their own folder: one copy of each, or contexts break.
      config: () => ({ resolve: { dedupe: ["solid-js", "@karkataka/runtime"] } }),
      async configureServer(server) {
        file = resolve(server.config.root, options.project);
        await readComponents();
        const karka = options.karka ?? process.env.KARKA;
        server.watcher.add(file);
        server.watcher.on("change", async (changed) => {
          if (resolve(changed) !== file) return;
          await readComponents();
          const module = server.moduleGraph.getModuleById("\0virtual:karka-components");
          if (module) server.moduleGraph.invalidateModule(module);
          server.ws.send({ type: "custom", event: "karka:project" });
        });
        server.middlewares.use((req, res, next) => {
          const path = req.url?.split("?")[0];
          const handle = (work: Promise<void>) => void work.catch(next);
          if (path === "/studio" && req.method === "GET") return handle(sendHtml(server, req, res, page("Karkataka Studio", app, '<div id="studio"></div>')));
          if (path === "/player" && req.method === "GET") return handle(sendHtml(server, req, res, page("Player", player, '<div id="player"></div>')));
          if (path === "/__karka/project" && req.method === "GET") return handle(sendProject(file, res));
          if (path === "/__karka/project" && req.method === "PUT") return handle(putProject(file, req, res));
          if (path === "/__karka/render" && req.method === "POST") return handle(render(server, file, karka, res));
          next();
        });
        server.httpServer?.once("listening", () =>
          setTimeout(() => server.resolvedUrls?.local[0] && server.config.logger.info(`  ➜  Studio:  ${server.resolvedUrls.local[0]}studio  (${options.project})`)),
        );
      },
    },
  ];
}

async function sendHtml(server: ViteDevServer, req: IncomingMessage, res: ServerResponse, html: string) {
  res.setHeader("Content-Type", "text/html");
  res.end(await server.transformIndexHtml(req.url!, html));
}

function sendJson(res: ServerResponse, status: number, body: unknown) {
  res.statusCode = status;
  res.setHeader("Content-Type", "application/json");
  res.end(JSON.stringify(body));
}

async function sendProject(file: string, res: ServerResponse) {
  try {
    const project = await loadProject(file);
    sendJson(res, 200, { project, dir: dirname(file), file: basename(file), problems: validate(project, filesNextTo(file)) } satisfies ProjectState);
  } catch (e) {
    sendJson(res, 500, { error: (e as Error).message });
  }
}

/** Save the Studio's edit, unless it is not a valid project. */
async function putProject(file: string, req: IncomingMessage, res: ServerResponse) {
  let project: Project;
  try {
    project = JSON.parse(await readBody(req));
  } catch {
    return sendJson(res, 400, { error: "the body is not JSON" });
  }
  const problems = validate(project);
  if (problems.length) return sendJson(res, 400, { error: "not saved: the project has problems", problems });
  await saveProject(file, project);
  sendJson(res, 200, { problems: validate(project, filesNextTo(file)) });
}

/** Render the project (as saved) with karka and stream `RenderEvent`s. Stops karka if the Studio goes away. */
async function render(server: ViteDevServer, file: string, karka: string | undefined, res: ServerResponse) {
  res.setHeader("Content-Type", "application/x-ndjson");
  const send = (event: RenderEvent) => res.write(JSON.stringify(event) + "\n");
  const stop = new AbortController();
  res.on("close", () => res.writableEnded || stop.abort());
  const name = basename(file, extname(file));
  const out = join(server.config.root, "out", `${name}.mp4`);
  try {
    send({ type: "building" });
    const result = await renderProject(file, out, { karka, signal: stop.signal, onProgress: (frame, frames) => send({ type: "progress", frame, frames }) });
    send({ type: "done", url: `/out/${name}.mp4?t=${Date.now()}`, result });
  } catch (e) {
    send({ type: "error", message: (e as Error).message });
  } finally {
    res.end();
  }
}

function readBody(req: IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    let body = "";
    req.on("data", (d) => (body += d));
    req.on("end", () => resolve(body));
    req.on("error", reject);
  });
}
