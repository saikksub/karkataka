import { render } from "solid-js/web";
import { afterEach, expect, test, vi } from "vitest";
import { config, getProps, SafeArea, Sequence, useFrame } from "./index.tsx";

// The engine drives every frame through this; tests do the same.
const setFrame = (n: number) => window.__setFrame!(n);

let dispose = () => {};
function mount(view: () => any) {
  const root = document.createElement("div");
  dispose = render(view, root);
  return root;
}
afterEach(() => {
  dispose();
  setFrame(0);
});

function Show(p: { id: string }) {
  const f = useFrame();
  return <i id={p.id}>{f()}</i>;
}

test("useFrame follows the frame the engine sets", () => {
  const root = mount(() => <Show id="a" />);
  expect(root.textContent).toBe("0");
  setFrame(42);
  expect(root.textContent).toBe("42");
});

test("Sequence shows children only in its window, counting from its start", () => {
  const root = mount(() => (
    <Sequence from={10} duration={5}>
      <Show id="s" />
    </Sequence>
  ));
  const shown = () => root.querySelector("#s")?.textContent ?? null;
  setFrame(9);
  expect(shown()).toBe(null);
  setFrame(10);
  expect(shown()).toBe("0");
  setFrame(14);
  expect(shown()).toBe("4");
  setFrame(15);
  expect(shown()).toBe(null);
});

test("Sequence without duration lasts to the end; nested starts add up", () => {
  const root = mount(() => (
    <Sequence from={10}>
      <Show id="outer" />
      <Sequence from={5} duration={2}>
        <Show id="inner" />
      </Sequence>
    </Sequence>
  ));
  const text = (id: string) => root.querySelector(`#${id}`)?.textContent ?? null;
  setFrame(14);
  expect([text("outer"), text("inner")]).toEqual(["4", null]);
  setFrame(16);
  expect([text("outer"), text("inner")]).toEqual(["6", "1"]);
  setFrame(10_000);
  expect([text("outer"), text("inner")]).toEqual(["9990", null]);
});

test("getProps overrides defaults with the props from the command line", () => {
  const saved = config.props;
  config.props = { question: "from cli" };
  expect(getProps({ question: "default", answer: 1 })).toEqual({ question: "from cli", answer: 1 });
  config.props = saved;
});

test("config comes from window.__karka, which karka render sets before the page's modules run", async () => {
  window.__karka = { props: { a: 1 }, frames: 90, fps: 30, width: 1080, height: 1920 };
  vi.resetModules();
  const fresh = await import("./index.tsx");
  expect(fresh.config).toBe(window.__karka);
  delete window.__karka;
  vi.resetModules();
  expect((await import("./index.tsx")).config).toEqual({ props: {}, frames: 300, fps: 30, width: 1080, height: 1920 });
});

test("SafeArea sits on the Reels safe zone", () => {
  const root = mount(() => <SafeArea>x</SafeArea>);
  const { top, right, bottom, left } = (root.firstChild as HTMLElement).style;
  expect([top, right, bottom, left]).toEqual(["250px", "200px", "440px", "96px"]);
});
