#!/usr/bin/env node
// Stands in for `karka render <page> --frames N --props JSON --out FILE --json --progress` in the plugin tests.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";

const args = process.argv.slice(2);
const opt = (name) => args[args.indexOf(name) + 1];
const [command, page] = args;
const frames = Number(opt("--frames"));
const props = JSON.parse(opt("--props"));
if (command !== "render" || !args.includes("--json") || !args.includes("--progress")) throw new Error(`unexpected args: ${args}`);
if (props.fail) {
  console.log(JSON.stringify({ ok: false, error: "the page broke" }));
  process.exit(1);
}
if (props.crash) {
  console.error("thread 'main' panicked");
  process.exit(101);
}
const built = readFileSync(page, "utf8").includes("fixture-scene");
console.error("some other log line");
for (let n = 1; n <= frames; n++) console.error(JSON.stringify({ frame: n, frames }));
mkdirSync(dirname(opt("--out")), { recursive: true });
writeFileSync(opt("--out"), "mp4");
console.log(JSON.stringify({ ok: true, result: { frames, props, built } }));
