import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import { Player } from "./Player.tsx";
import type { ComponentModule, PlayerProject } from "./types.ts";

// The page sets the project (a render), or the Studio sends it and changes it (__karkaSetProject).
// Component modules are imported through the page's import map; the page is ready once they are.
const components: Record<string, ComponentModule> = {};
window.__karkaComponents = components;

async function load(project: PlayerProject) {
  const keys = project.tracks.flatMap((t) => t.clips.flatMap((c) => (c.type === "component" && c.module ? [c.module] : [])));
  await Promise.all(keys.filter((k) => !(k in components)).map(async (k) => (components[k] = await import(/* @vite-ignore */ k))));
}

const [project, setProject] = createSignal<PlayerProject | null>(null);
window.__karkaSetProject = (p) => void load(p).then(() => setProject(p), window.__karkaFailed);

const start = window.__karkaProject ? load(window.__karkaProject).then(() => setProject(window.__karkaProject!)) : Promise.resolve();
start.then(() => {
  render(
    () => (
      <Show when={project()}>
        {(p) => {
          // Sized in px, not vh: karka's view is 1 px taller than the video (its frame-marker row).
          document.documentElement.style.width = document.body.style.width = `${p().width}px`;
          document.documentElement.style.height = document.body.style.height = `${p().height}px`;
          return <Player project={p()} components={components} />;
        }}
      </Show>
    ),
    document.getElementById("player")!,
  );
  window.__karkaBooted?.();
}, window.__karkaFailed);
