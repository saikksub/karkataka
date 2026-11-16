import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import components from "virtual:karka-components";
import built from "virtual:karka-project";
import { Player } from "./Player.tsx";
import type { PlayerProject } from "./types.ts";

// A render build carries its project; in the Studio the project arrives (and changes) through __karkaSetProject.
const [project, setProject] = createSignal<PlayerProject | null>(built);
window.__karkaSetProject = (p) => setProject(p);
window.__karkaComponents = components;

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
