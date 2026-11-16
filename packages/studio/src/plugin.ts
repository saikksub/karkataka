import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import type { IncomingMessage, ServerResponse } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { build, normalizePath, type Plugin, type ViteDevServer } from "vite";

export interface StudioOptions {
  /** The karka executable. Default: `$KARKA`, else `karka` on PATH. */
  karka?: string;
}

/** A message on the render stream (one JSON object per line). */
export type RenderEvent =
  | { type: "building" }
  | { type: "progress"; frame: number; frames: number }
  | { type: "done"; url: string; result: unknown }
  | { type: "error"; message: string };

// The Studio app is compiled by the scene's own dev server (it has vite-plugin-solid, as every scene does).
// shortcut: works while the studio package is a workspace source package; a published one ships built JS.
const app = normalizePath(fileURLToPath(new URL("./app/main.tsx", import.meta.url)));

/** Serves the Studio at /studio in the scene's Vite dev server, and renders with karka at POST /__karka/render. */
export function studio(options: StudioOptions = {}): Plugin {
  return {
    name: "karkataka-studio",
    apply: "serve",
    configureServer(server) {
      const karka = options.karka ?? process.env.KARKA ?? "karka";
      server.middlewares.use((req, res, next) => {
        const path = req.url?.split("?")[0];
        if (path === "/studio" && req.method === "GET") return void sendStudio(server, req, res).catch(next);
        if (path === "/__karka/render" && req.method === "POST") return void render(server, karka, req, res);
        next();
      });
      server.httpServer?.once("listening", () =>
        setTimeout(() => server.resolvedUrls?.local[0] && server.config.logger.info(`  ➜  Studio:  ${server.resolvedUrls.local[0]}studio`)),
      );
    },
  };
}

async function sendStudio(server: ViteDevServer, req: IncomingMessage, res: ServerResponse) {
  const html = `<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Karkataka Studio</title><link rel="icon" href="data:,"></head>
<body><div id="studio"></div><script type="module" src="/@fs/${app.replace(/^\//, "")}"></script></body></html>`;
  res.setHeader("Content-Type", "text/html");
  res.end(await server.transformIndexHtml(req.url!, html));
}

/** Build the scene into one HTML file, render it with karka, and stream `RenderEvent`s as it goes. */
async function render(server: ViteDevServer, karka: string, req: IncomingMessage, res: ServerResponse) {
  res.setHeader("Content-Type", "application/x-ndjson");
  const send = (event: RenderEvent) => res.write(JSON.stringify(event) + "\n");
  const dist = await mkdtemp(join(tmpdir(), "karka-studio-"));
  try {
    const { props, frames } = JSON.parse(await readBody(req));
    if (!Number.isInteger(frames) || frames < 1) throw new Error("frames must be a whole number, at least 1");
    if (typeof props !== "object" || props === null || Array.isArray(props)) throw new Error("props must be an object");
    send({ type: "building" });
    const root = server.config.root;
    await build({ root, configFile: server.config.configFile, mode: "production", logLevel: "warn", build: { outDir: dist, emptyOutDir: true } });
    const out = join(root, "out", "studio.mp4");
    const args = ["render", join(dist, "index.html"), "--frames", String(frames), "--props", JSON.stringify(props), "--out", out, "--json", "--progress"];
    const result = await runKarka(karka, args, res, (frame) => send({ type: "progress", frame, frames }));
    send({ type: "done", url: `/out/studio.mp4?t=${Date.now()}`, result });
  } catch (e) {
    send({ type: "error", message: e instanceof Error ? e.message : String(e) });
  } finally {
    await rm(dist, { recursive: true, force: true });
    res.end();
  }
}

/** Run karka, report each `--progress` line, and resolve with its `--json` result. Killed if the Studio disconnects. */
function runKarka(karka: string, args: string[], res: ServerResponse, onFrame: (frame: number) => void): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const child = spawn(karka, args, { stdio: ["ignore", "pipe", "pipe"] });
    res.on("close", () => res.writableEnded || child.kill());
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (d) => (stdout += d));
    createInterface({ input: child.stderr }).on("line", (line) => {
      const frame = progressFrame(line);
      if (frame === undefined) stderr += line + "\n";
      else onFrame(frame);
    });
    child.on("error", (e) => reject(new Error(`cannot run karka (${karka}): ${e.message}. Set KARKA or pass studio({ karka }).`)));
    child.on("close", () => {
      try {
        const out = JSON.parse(stdout.trim().split("\n").pop()!);
        if (out.ok) resolve(out.result);
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

function readBody(req: IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    let body = "";
    req.on("data", (d) => (body += d));
    req.on("end", () => resolve(body));
    req.on("error", reject);
  });
}
