// Compiling scene files (TypeScript + SolidJS JSX) to ES modules, in the browser, without a bundler.
// Imports between files become `@project/<path>` keys, which the page's import map points at each
// compiled module (see boot.ts).

import { transform } from "@babel/standalone";
import solid from "babel-preset-solid";

/** Packages a scene can import: they are built into karka. */
export const PACKAGES = ["solid-js", "solid-js/web", "solid-js/store", "@karkataka/runtime"];
/** Files compiled as modules; any other file imported is an asset whose default export is its URL. */
export const CODE = [".tsx", ".ts", ".jsx", ".js", ".json"];
/** The import key of a project file: `@project/` and its path from the project folder. */
export const key = (path: string) => `@project/${path}`;

const dir = (path: string) => path.slice(0, path.lastIndexOf("/") + 1);

/** `relative` (`./a`, `../b/c`) from the folder `from`, with `.` and `..` worked out. */
function join(from: string, relative: string): string {
  const parts: string[] = [];
  for (const part of (from + relative).split("/")) {
    if (part === "..") parts.pop();
    else if (part !== "." && part !== "") parts.push(part);
  }
  return parts.join("/");
}

export class CompileError extends Error {}

/**
 * What `spec`, imported by the file `from`, is: a package, a project code file (`files` lists
 * them; the extension and `/index` may be left out), or an asset (a file with another extension).
 */
export function resolve(spec: string, from: string, files: Set<string>): { kind: "package" | "code" | "asset"; path: string } {
  if (PACKAGES.includes(spec)) return { kind: "package", path: spec };
  if (!spec.startsWith("./") && !spec.startsWith("../")) {
    throw new CompileError(`${from}: cannot import "${spec}". Scenes can import ${PACKAGES.join(", ")} and files in the project ("./file").`);
  }
  const path = join(dir(from), spec);
  for (const candidate of [path, ...CODE.map((e) => path + e), ...CODE.map((e) => `${path}/index${e}`)]) {
    if (files.has(candidate)) return { kind: "code", path: candidate };
  }
  const ext = path.slice(path.lastIndexOf("."));
  if (path.includes(".") && !CODE.includes(ext)) return { kind: "asset", path };
  throw new CompileError(`${from}: cannot import "${spec}": no file ${path} (or with ${CODE.join(", ")}) in the project`);
}

/**
 * Compile one project file to an ES module. `files` lists the project's code files; code files it
 * imports are added to `deps`, asset paths to `assets`.
 */
export function compile(path: string, source: string, files: Set<string>, assets: Set<string>, deps = new Set<string>()): string {
  if (path.endsWith(".json")) {
    try {
      return `export default ${JSON.stringify(JSON.parse(source))};`;
    } catch (e) {
      throw new CompileError(`${path}: not valid JSON: ${(e as Error).message}`);
    }
  }
  const rewrite = (spec: string) => {
    const target = resolve(spec, path, files);
    if (target.kind === "asset") assets.add(target.path);
    if (target.kind === "code") deps.add(target.path);
    return target.kind === "package" ? spec : key(target.path);
  };
  type Source = { node: { source?: { value: string } | null } };
  const sourceOf = (p: Source) => {
    if (p.node.source) p.node.source.value = rewrite(p.node.source.value);
  };
  const imports = () => ({
    visitor: {
      ImportDeclaration: sourceOf,
      ExportNamedDeclaration: sourceOf,
      ExportAllDeclaration: sourceOf,
      // import("./file"): Babel 8 calls it an ImportExpression.
      ImportExpression(p: { node: { source: { type: string; value: string } } }) {
        if (p.node.source.type === "StringLiteral") p.node.source.value = rewrite(p.node.source.value);
      },
    },
  });
  try {
    const result = transform(source, {
      filename: path,
      sourceType: "module",
      presets: ["typescript", [solid, { generate: "dom" }]],
      plugins: [imports as never],
    });
    return result?.code ?? "";
  } catch (e) {
    if (e instanceof CompileError) throw e;
    // Babel's message starts with the file name and has a code frame; keep it as it is.
    throw new CompileError((e as Error).message);
  }
}
