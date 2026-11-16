// The page's boot script (a classic script, so it runs before any module): compile the project's
// files, point an import map at the modules, then start the player. Errors go to the engine as a
// `__karka_error:` console message (it stops the render with it) and onto the page.

import { compile, key } from "./compile.ts";

export interface Boot {
  /** Project code files by path from the project folder ("scenes/Intro.tsx"). */
  sources: Record<string, string>;
  /** The scene files the project uses. Only they and what they import are compiled, so other code in the folder (tests, tool configs) is left alone. */
  entries: string[];
  /** Built-in modules by import name ("solid-js"), as ES module source text. */
  modules: Record<string, string>;
  /** The module to start ("@karkataka/player"). */
  main: string;
  /** URL of the project folder, ending in "/": where imported assets are found. */
  base: string;
}

declare global {
  interface Window {
    /** Settles when the page is ready to draw frames; karka waits for it. */
    __karkaReady?: Promise<void>;
    __karkaBooted?: () => void;
    __karkaFailed?: (error: unknown) => void;
  }
}

const url = (code: string) => URL.createObjectURL(new Blob([code], { type: "text/javascript" }));

export function boot(options: Boot): void {
  window.__karkaReady = new Promise<void>((resolve, reject) => {
    window.__karkaBooted = resolve;
    window.__karkaFailed = (error) => {
      fail(error);
      reject(error);
    };
  });
  window.__karkaReady.catch(() => {});
  const imports: Record<string, string> = {};
  try {
    for (const [name, code] of Object.entries(options.modules)) imports[name] = url(code);
    const files = new Set(Object.keys(options.sources));
    const assets = new Set<string>();
    const queue = [...options.entries];
    for (let path = queue.pop(); path !== undefined; path = queue.pop()) {
      if (key(path) in imports) continue;
      if (!files.has(path)) throw new Error(`scene file not found: ${path}`);
      const deps = new Set<string>();
      imports[key(path)] = url(compile(path, options.sources[path], files, assets, deps));
      queue.push(...deps);
    }
    for (const path of assets) imports[key(path)] = url(`export default ${JSON.stringify(options.base + path)};`);
  } catch (e) {
    return window.__karkaFailed!(e);
  }
  const map = document.createElement("script");
  map.type = "importmap";
  map.textContent = JSON.stringify({ imports });
  document.head.append(map);
  const start = document.createElement("script");
  start.type = "module";
  start.textContent = `import ${JSON.stringify(options.main)};`;
  document.head.append(start);
  window.addEventListener("error", (e) => window.__karkaFailed!(e.error ?? e.message));
  window.addEventListener("unhandledrejection", (e) => window.__karkaFailed!(e.reason));
}

/** Tell the engine (and whoever looks at the page) what went wrong. */
function fail(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  console.log(`__karka_error:${message}`);
  const box = document.createElement("pre");
  box.style.cssText = "position:fixed;inset:0;margin:0;padding:24px;background:#300;color:#fff;font:16px/1.4 Menlo,monospace;white-space:pre-wrap;z-index:2147483646";
  box.textContent = message;
  document.body.append(box);
}
