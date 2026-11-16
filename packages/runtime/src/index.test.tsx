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

test("the report lists every Audio clip, with Sequence starts and ends applied", async () => {
  vi.resetModules();
  const { Audio, Sequence } = await import("./index.tsx");
  const root = document.createElement("div");
  const stop = render(
    () => (
      <>
        <Audio src="music.mp3" volume={0.3} />
        <Sequence from={100} duration={50}>
          <Audio src="tick.wav" start={10} />
          <Audio src="long.wav" trim={[30, 300]} />
          <Audio src="short.wav" trim={[0, 20]} start={5} />
        </Sequence>
      </>
    ),
    root,
  );
  const report = JSON.parse(window.__karkaReport!());
  stop();
  expect(report.audio).toEqual([
    { src: "music.mp3", from: 0, to: null, skip: 0, volume: 0.3 },
    // Inside the Sequence: offset by its start, cut at its end (frame 150).
    { src: "tick.wav", from: 110, to: 150, skip: 0, volume: 1 },
    { src: "long.wav", from: 100, to: 150, skip: 30, volume: 1 },
    { src: "short.wav", from: 105, to: 125, skip: 0, volume: 1 },
  ]);
});

test("Audio: trim with only a start plays to the end; a remounted Sequence adds no duplicates; the report leaves frame 0", async () => {
  vi.resetModules();
  const { Audio, Sequence, useFrame } = await import("./index.tsx");
  const root = document.createElement("div");
  const Frame = () => {
    const f = useFrame();
    return <b>{f()}</b>;
  };
  const stop = render(
    () => (
      <>
        <Frame />
        <Audio src="bed.mp3" trim={[45]} volume={0.2} />
        {/* Frame 12 below mounts the first inner Sequence; the report's walk mounts it again. */}
        <Sequence from={0} duration={300}>
          <Sequence from={10} duration={5}>
            <Audio src="pop.wav" />
          </Sequence>
          <Sequence from={30} duration={5}>
            <Audio src="pop.wav" start={2} />
          </Sequence>
        </Sequence>
        {/* Starts after its Sequence ends: to <= from, which karka drops. */}
        <Sequence from={50} duration={5}>
          <Audio src="late.wav" start={8} />
        </Sequence>
      </>
    ),
    root,
  );
  window.__setFrame!(12);
  const { audio } = JSON.parse(window.__karkaReport!());
  expect(root.textContent).toBe("0");
  stop();
  expect(audio).toEqual([
    { src: "bed.mp3", from: 0, to: null, skip: 45, volume: 0.2 },
    { src: "pop.wav", from: 10, to: 15, skip: 0, volume: 1 },
    { src: "pop.wav", from: 32, to: 35, skip: 0, volume: 1 },
    { src: "late.wav", from: 58, to: 55, skip: 0, volume: 1 },
  ]);
});

test("Audio inside a Sequence that is never shown (past the last frame) is not reported", async () => {
  vi.resetModules();
  const { Audio, Sequence, config } = await import("./index.tsx");
  const stop = render(
    () => (
      <Sequence from={config.frames + 10}>
        <Audio src="never.wav" />
      </Sequence>
    ),
    document.createElement("div"),
  );
  expect(JSON.parse(window.__karkaReport!()).audio).toEqual([]);
  stop();
});

test("outside karka, the Studio's ?karka= URL sets props and frames; bad JSON is ignored", async () => {
  const load = async (search: string) => {
    window.history.replaceState(null, "", `/${search}`);
    vi.resetModules();
    return (await import("./index.tsx")).config;
  };
  expect(await load(`?karka=${encodeURIComponent(JSON.stringify({ props: { a: 2 }, frames: 90 }))}`)).toEqual({
    props: { a: 2 },
    frames: 90,
    fps: 30,
    width: 1080,
    height: 1920,
  });
  expect((await load("?karka=%7Bbroken")).frames).toBe(300);
  window.history.replaceState(null, "", "/");
});

test("getProps tells the Studio the defaults and schema", () => {
  const schema = { title: { type: "string" as const } };
  getProps({ title: "x" }, schema);
  expect(window.__karkaScene).toEqual({ defaults: { title: "x" }, schema });
});
