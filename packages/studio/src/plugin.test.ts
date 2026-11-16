import { existsSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createServer, type ViteDevServer } from "vite";
import solid from "vite-plugin-solid";
import { afterAll, beforeAll, expect, test } from "vitest";
import { progressFrame, studio, type RenderEvent } from "./plugin.ts";

const fixture = fileURLToPath(new URL("../test/fixture", import.meta.url));
const fakeKarka = fileURLToPath(new URL("../test/fake-karka.mjs", import.meta.url));

async function serve(karka: string) {
  const server = await createServer({ root: fixture, configFile: false, logLevel: "silent", plugins: [solid(), studio({ karka })], server: { port: 0 } });
  await server.listen();
  return { server, base: server.resolvedUrls!.local[0] };
}

let server: ViteDevServer;
let base: string;
beforeAll(async () => ({ server, base } = await serve(fakeKarka)));
afterAll(async () => {
  await server.close();
  rmSync(`${fixture}/out`, { recursive: true, force: true });
});

async function render(url: string, body: unknown): Promise<RenderEvent[]> {
  const res = await fetch(`${url}__karka/render`, { method: "POST", body: JSON.stringify(body) });
  expect(res.headers.get("content-type")).toBe("application/x-ndjson");
  return (await res.text()).trim().split("\n").map((l) => JSON.parse(l));
}

test("serves the Studio page, and its app compiles", async () => {
  const html = await (await fetch(`${base}studio`)).text();
  expect(html).toContain('<div id="studio">');
  expect(html).toContain("/@vite/client");
  const app = html.match(/src="(\/@fs\/[^"]+main\.tsx)"/)![1];
  const module = await fetch(base + app.slice(1));
  expect(module.status).toBe(200);
  expect(await module.text()).toContain("App");
  // Other paths are left to Vite.
  expect(await (await fetch(base)).text()).toContain("fixture-scene");
});

test("a render builds the scene, streams progress, and ends with the video", async () => {
  const events = await render(base, { props: { title: "hi" }, frames: 3 });
  expect(events.slice(0, 4)).toEqual([
    { type: "building" },
    { type: "progress", frame: 1, frames: 3 },
    { type: "progress", frame: 2, frames: 3 },
    { type: "progress", frame: 3, frames: 3 },
  ]);
  const done = events[4] as Extract<RenderEvent, { type: "done" }>;
  expect(done.type).toBe("done");
  expect(done.url).toMatch(/^\/out\/studio\.mp4\?t=\d+$/);
  // karka got the built page (the fixture's HTML), the props and the frame count.
  expect(done.result).toEqual({ frames: 3, props: { title: "hi" }, built: true });
  expect(events).toHaveLength(5);
  expect(existsSync(`${fixture}/out/studio.mp4`)).toBe(true);
  expect((await fetch(base + done.url.slice(1))).status).toBe(200);
});

test("bad requests and failed renders end with an error event", async () => {
  const error = async (body: unknown) => (await render(base, body)).at(-1);
  expect(await error({ props: {}, frames: 0 })).toEqual({ type: "error", message: "frames must be a whole number, at least 1" });
  expect(await error({ props: [], frames: 2 })).toEqual({ type: "error", message: "props must be an object" });
  expect((await error("not json")) as { type: string }).toMatchObject({ type: "error" });
  expect(await error({ props: { fail: true }, frames: 2 })).toEqual({ type: "error", message: "the page broke" });
  expect(await error({ props: { crash: true }, frames: 2 })).toEqual({ type: "error", message: "thread 'main' panicked" });
});

test("a missing karka says how to point the Studio at one", async () => {
  const missing = await serve("/nonexistent/karka");
  try {
    const last = (await render(missing.base, { props: {}, frames: 1 })).at(-1) as { type: string; message: string };
    expect(last.type).toBe("error");
    expect(last.message).toMatch(/^cannot run karka \(\/nonexistent\/karka\).*Set KARKA/);
  } finally {
    await missing.server.close();
  }
});

test("progressFrame reads karka's --progress lines only", () => {
  expect(progressFrame('{"frame":12,"frames":90}')).toBe(12);
  expect(progressFrame("warning: something")).toBeUndefined();
  expect(progressFrame('{"other":1}')).toBeUndefined();
  expect(progressFrame("null")).toBeUndefined();
});
