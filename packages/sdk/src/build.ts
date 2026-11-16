// Building the player page for a project: the player app, the project's component modules and
// (for a render) the project itself, all in one HTML file.

import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { build, type Plugin } from "vite";
import solid from "vite-plugin-solid";
import { viteSingleFile } from "vite-plugin-singlefile";
import { resolveIn } from "./file.ts";
import type { PlayerProject } from "./player/types.ts";
import type { Project } from "./project.ts";

/** The player app: index.html and main.tsx. */
export const PLAYER_DIR = fileURLToPath(new URL("./player/", import.meta.url));

/** Component clips' modules: the path as written in the project, and as an absolute path. */
export function componentsOf(projectPath: string, project: Project): Record<string, string> {
  const paths = project.tracks.flatMap((t) => t.clips.flatMap((c) => (c.type === "component" ? [c.component] : [])));
  return Object.fromEntries(paths.map((p) => [p, resolveIn(projectPath, p)]));
}

/** Provides `virtual:karka-project` and `virtual:karka-components` to the player; `get` is read on every load. */
export function playerModules(get: () => { components: Record<string, string>; project: PlayerProject | null }): Plugin {
  return {
    name: "karkataka-player",
    resolveId(id) {
      if (id === "virtual:karka-project" || id === "virtual:karka-components") return "\0" + id;
    },
    load(id) {
      if (id === "\0virtual:karka-project") return `export default ${JSON.stringify(get().project)};`;
      if (id === "\0virtual:karka-components") {
        const entries = Object.entries(get().components);
        const imports = entries.map(([, path], i) => `import * as m${i} from ${JSON.stringify(path)};`);
        return `${imports.join("\n")}\nexport default {${entries.map(([name], i) => `${JSON.stringify(name)}: m${i}`).join(", ")}};`;
      }
    },
  };
}

/** Build the player with `project` built in, into `outDir`. Returns the page's path. */
export async function buildPlayer(projectPath: string, project: PlayerProject, outDir: string): Promise<string> {
  const components = componentsOf(projectPath, project as unknown as Project);
  await build({
    root: PLAYER_DIR,
    configFile: false,
    logLevel: "warn",
    plugins: [solid(), viteSingleFile(), playerModules(() => ({ components, project }))],
    // Components import solid-js and the runtime from their own folder: one copy of each, or contexts break.
    resolve: { dedupe: ["solid-js", "@karkataka/runtime"] },
    build: { outDir, emptyOutDir: true },
  });
  return join(outDir, "index.html");
}
