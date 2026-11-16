import { expect, test } from "vitest";
import { compile, CompileError, resolve } from "./compile.ts";

const files = new Set(["scenes/Intro.tsx", "scenes/lib/util.ts", "scenes/parts/index.tsx", "data.json"]);

test("imports resolve to packages, project code files and assets", () => {
  expect(resolve("solid-js", "scenes/Intro.tsx", files)).toEqual({ kind: "package", path: "solid-js" });
  expect(resolve("./lib/util", "scenes/Intro.tsx", files)).toEqual({ kind: "code", path: "scenes/lib/util.ts" });
  expect(resolve("./parts", "scenes/Intro.tsx", files)).toEqual({ kind: "code", path: "scenes/parts/index.tsx" });
  expect(resolve("../data.json", "scenes/Intro.tsx", files)).toEqual({ kind: "code", path: "data.json" });
  expect(resolve("../media/logo.png", "scenes/Intro.tsx", files)).toEqual({ kind: "asset", path: "media/logo.png" });
  expect(() => resolve("lodash", "scenes/Intro.tsx", files)).toThrow('scenes/Intro.tsx: cannot import "lodash". Scenes can import solid-js');
  expect(() => resolve("./nope", "scenes/Intro.tsx", files)).toThrow("no file scenes/nope");
  expect(() => resolve("/abs.ts", "scenes/Intro.tsx", files)).toThrow(CompileError);
});

test("TSX compiles to a Solid module with project imports as @project keys", () => {
  const assets = new Set<string>();
  const source = `
    import { createSignal } from "solid-js";
    import { useFrame } from "@karkataka/runtime";
    import { double } from "./lib/util";
    import logo from "../media/logo.png";
    export { double } from "./lib/util";
    const later = () => import("./parts");
    type Props = { title: string };
    export default function Intro(props: Props) {
      const f = useFrame();
      return <h1 class="t" title={props.title}><img src={logo} />{double(f())}</h1>;
    }`;
  const deps = new Set<string>();
  const code = compile("scenes/Intro.tsx", source, files, assets, deps);
  expect([...deps].sort()).toEqual(["scenes/lib/util.ts", "scenes/parts/index.tsx"]);
  expect(code).toContain('from "solid-js/web"');
  expect(code).toContain('from "@project/scenes/lib/util.ts"');
  expect(code).toContain('import logo from "@project/media/logo.png"');
  expect(code).toContain('import("@project/scenes/parts/index.tsx")');
  expect(code).not.toContain("Props");
  // JSX became Solid's DOM template code.
  expect(code).toContain("_$template(");
  expect(code).not.toContain("title={props.title}");
  expect([...assets]).toEqual(["media/logo.png"]);
});

test("JSON becomes a module; syntax errors say where", () => {
  expect(compile("data.json", '{"a": [1]}', files, new Set())).toBe('export default {"a":[1]};');
  expect(() => compile("data.json", "{", files, new Set())).toThrow("data.json: not valid JSON");
  expect(() => compile("scenes/Intro.tsx", "export default () => <div>", files, new Set())).toThrow(/scenes\/Intro\.tsx: .*\(1:/);
  expect(() => compile("scenes/Intro.tsx", 'import x from "react";', files, new Set())).toThrow(/cannot import "react"/);
});
