#!/usr/bin/env node
// Stands in for `karka render <page> --frames N --width W --height H --fps F --out FILE --json --progress` in the plugin tests.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

const args = process.argv.slice(2);
const opt = (name) => args[args.indexOf(name) + 1];
const [command, page] = args;
const frames = Number(opt("--frames"));
if (command !== "render" || !args.includes("--json") || !args.includes("--progress")) throw new Error(`unexpected args: ${args}`);
const html = readFileSync(page, "utf8");
if (html.includes("fail-render")) {
  console.log(JSON.stringify({ ok: false, error: "the page broke" }));
  process.exit(1);
}
console.error("some other log line");
for (let n = 1; n <= frames; n++) console.error(JSON.stringify({ frame: n, frames }));
mkdirSync(dirname(opt("--out")), { recursive: true });
writeFileSync(opt("--out"), "mp4");
const size = `${opt("--width")}x${opt("--height")}@${opt("--fps")}`;
// The built page carries the project: its text and the component module.
console.log(JSON.stringify({ ok: true, result: { frames, size, text: html.includes("fixture-text"), component: html.includes("fixture-card") } }));
