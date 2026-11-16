import { cpSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer, type ViteDevServer } from "vite";
import solid from "vite-plugin-solid";
import { afterAll, beforeAll, expect, test } from "vitest";
import { studio, type RenderEvent } from "./plugin.ts";

// A copy of the fixture, so saves do not touch the repo.
const dir = mkdtempSync(join(tmpdir(), "karka-studio-"));
cpSync(fileURLToPath(new URL("../test/fixture", import.meta.url)), dir, { recursive: true });
const file = join(dir, "project.json");
const original = readFileSync(file, "utf8");
const fakeKarka = fileURLToPath(new URL("../test/fake-karka.mjs", import.meta.url));

async function serve(karka: string) {
  const server = await createServer({ root: dir, configFile: false, logLevel: "silent", plugins: [solid(), studio({ project: "project.json", karka })], server: { port: 0 } });
  await server.listen();
  return { server, base: server.resolvedUrls!.local[0] };
}

let server: ViteDevServer;
let base: string;
beforeAll(async () => ({ server, base } = await serve(fakeKarka)));
afterAll(async () => {
  await server.close();
  rmSync(dir, { recursive: true, force: true });
});

const json = async (path: string, init?: RequestInit) => {
  const res = await fetch(base + path, init);
  return { status: res.status, body: await res.json() };
};
async function render(url: string): Promise<RenderEvent[]> {
  const res = await fetch(`${url}__karka/render`, { method: "POST" });
  expect(res.headers.get("content-type")).toBe("application/x-ndjson");
  return (await res.text()).trim().split("\n").map((l) => JSON.parse(l));
}

test("serves the Studio and the player; both compile, with the project's components", async () => {
  for (const [path, root] of [["studio", '<div id="studio">'], ["player", '<div id="player">']]) {
    const html = await (await fetch(base + path)).text();
    expect(html).toContain(root);
    expect(html).toContain("/@vite/client");
    const module = await fetch(base + html.match(/src="\/(@fs\/[^"]+main\.tsx)"/)![1]);
    expect(module.status).toBe(200);
  }
  const components = await (await fetch(`${base}@id/__x00__virtual:karka-components`)).text();
  expect(components).toContain("Card.tsx");
});

test("GET /__karka/project gives the project, its folder and its problems", async () => {
  const { body } = await json("__karka/project");
  expect(body.project.tracks.map((t: { id: string }) => t.id)).toEqual(["V1", "T1"]);
  expect(body.dir).toBe(realpathSync(dir));
  expect(body.file).toBe("project.json");
  expect(body.problems).toEqual([]);
});

test("PUT saves a valid project and refuses a broken one", async () => {
  const project = JSON.parse(original);
  project.tracks[1].clips[0].text = "saved";
  expect(await json("__karka/project", { method: "PUT", body: JSON.stringify(project) })).toEqual({ status: 200, body: { problems: [] } });
  expect(JSON.parse(readFileSync(file, "utf8")).tracks[1].clips[0].text).toBe("saved");
  project.tracks[1].clips[0].duration = 0;
  const refused = await json("__karka/project", { method: "PUT", body: JSON.stringify(project) });
  expect(refused.status).toBe(400);
  expect(refused.body.problems[0]).toEqual({ path: 'tracks[1].clips[0] ("title").duration', message: "must be a whole number of frames, at least 1" });
  expect((await json("__karka/project", { method: "PUT", body: "{" })).status).toBe(400);
  expect(JSON.parse(readFileSync(file, "utf8")).tracks[1].clips[0].text).toBe("saved");
  writeFileSync(file, original);
});

test("a change to the file on disk tells the Studio to reload the project", async () => {
  const ws = new WebSocket(base.replace("http", "ws"), "vite-hmr");
  await new Promise((r) => ws.addEventListener("open", r, { once: true }));
  const told = new Promise<string>((resolve) =>
    ws.addEventListener("message", (m) => String(m.data).includes("karka:project") && resolve(String(m.data))),
  );
  writeFileSync(file, original.replace("fixture-text", "fixture-text "));
  expect(JSON.parse(await told)).toMatchObject({ type: "custom", event: "karka:project" });
  ws.close();
  writeFileSync(file, original);
});

test("a render builds the player with the project and streams progress", async () => {
  const events = await render(base);
  expect(events.slice(0, 3)).toEqual([{ type: "building" }, { type: "progress", frame: 1, frames: 75 }, { type: "progress", frame: 2, frames: 75 }]);
  const done = events.at(-1) as Extract<RenderEvent, { type: "done" }>;
  expect(done).toMatchObject({ type: "done", url: expect.stringMatching(/^\/out\/project\.mp4\?t=\d+$/) });
  expect(done.result).toEqual({ frames: 75, size: "640x360@25", text: true, component: true });
  expect(events).toHaveLength(77);
  expect((await fetch(base + done.url.slice(1))).status).toBe(200);
});

test("render errors: a broken project, a failing karka, a missing karka", async () => {
  const broken = JSON.parse(original);
  broken.tracks[0].clips[1].component = "Gone.tsx";
  writeFileSync(file, JSON.stringify(broken));
  expect((await render(base)).at(-1)).toEqual({ type: "error", message: expect.stringContaining("file not found: Gone.tsx") });
  writeFileSync(file, original.replace("fixture-text", "fail-render"));
  expect((await render(base)).at(-1)).toEqual({ type: "error", message: "the page broke" });
  writeFileSync(file, original);
  const missing = await serve("/nonexistent/karka");
  try {
    const last = (await render(missing.base)).at(-1) as { type: string; message: string };
    expect(last.message).toMatch(/^cannot run karka \(\/nonexistent\/karka\).*Set KARKA/);
  } finally {
    await missing.server.close();
  }
});
