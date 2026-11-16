// Builds the JavaScript the karka binary carries (crates/karka/assets/), so installing karka needs no
// Node: run `pnpm assets` after changing the runtime, player, compiler or Studio, and commit the result.

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { build, type InlineConfig } from "vite";
import solid from "vite-plugin-solid";

const root = fileURLToPath(new URL("../..", import.meta.url));
const out = join(root, "crates/karka/assets");
mkdirSync(out, { recursive: true });
const packages = ["solid-js", "solid-js/web", "solid-js/store", "@karkataka/runtime"];

/** One ES module (or IIFE), with the built-in packages left as imports for the page's import map. */
const bundle = (entry: string, file: string, extra: Partial<InlineConfig> = {}) =>
  build({
    configFile: false,
    logLevel: "warn",
    plugins: [solid()],
    define: { "process.env.NODE_ENV": '"production"' },
    build: {
      outDir: out,
      emptyOutDir: false,
      minify: true,
      lib: { entry: join(root, entry), formats: ["es"], fileName: () => file },
      rollupOptions: { external: packages },
    },
    ...extra,
  });

await bundle("packages/runtime/src/index.tsx", "runtime.js");
await bundle("packages/player/src/main.tsx", "player.js");
await build({
  configFile: false,
  logLevel: "warn",
  define: { "process.env.NODE_ENV": '"production"' },
  resolve: { alias: { assert: join(root, "packages/compiler/src/assert.cjs") } },
  build: {
    outDir: out,
    emptyOutDir: false,
    minify: true,
    lib: { entry: join(root, "packages/compiler/src/index.ts"), formats: ["iife"], name: "KarkaCompiler", fileName: () => "compiler.js" },
    // Babel reads Node's `process` here and there; a page has none.
    rollupOptions: { output: { banner: 'var process = { env: { NODE_ENV: "production" }, cwd: function () { return "/"; }, platform: "browser", version: "", versions: {} };' } },
  },
});
// The Studio app: one script and one stylesheet, served by `karka studio`.
await build({
  configFile: false,
  logLevel: "warn",
  plugins: [solid()],
  define: { "process.env.NODE_ENV": '"production"' },
  build: {
    outDir: out,
    emptyOutDir: false,
    minify: true,
    lib: { entry: join(root, "packages/studio/src/app/main.tsx"), formats: ["iife"], name: "KarkaStudio", fileName: () => "studio.js", cssFileName: "studio" },
  },
});
// solid-js ships browser ES modules; each imports the others by name, which the import map resolves.
const solidDir = dirname(createRequire(import.meta.url).resolve("solid-js/package.json"));
for (const [from, to] of [["dist/solid.js", "solid.js"], ["web/dist/web.js", "solid-web.js"], ["store/dist/store.js", "solid-store.js"]]) {
  copyFileSync(join(solidDir, from), join(out, to));
}
// Types for editors, written into each project's .karka/types/ by `karka new` (see the Rust cli::types).
const types: Record<string, string> = {};
const collect = (dir: string, prefix: string) => {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) collect(path, `${prefix}${name}/`);
    else if (name.endsWith(".d.ts")) types[prefix + name] = readFileSync(path, "utf8");
  }
};
for (const dir of ["types", "web/types", "store/types"]) collect(join(solidDir, dir), `solid-js/${dir}/`);
// solid-js types import csstype for CSS property names.
const csstype = dirname(createRequire(join(solidDir, "package.json")).resolve("csstype/package.json"));
types["csstype/index.d.ts"] = readFileSync(join(csstype, "index.d.ts"), "utf8");
const declarations = mkdtempSync(join(tmpdir(), "karka-types-"));
execFileSync(join(root, "node_modules/.bin/tsc"), ["-p", join(root, "packages/runtime"), "--noEmit", "false", "--declaration", "--emitDeclarationOnly", "--rewriteRelativeImportExtensions", "--outDir", declarations], { stdio: "inherit" });
collect(declarations, "runtime/");
rmSync(declarations, { recursive: true });
writeFileSync(join(out, "types.json"), JSON.stringify(types));
console.log(`assets in ${out} (${Object.keys(types).length} type files)`);
