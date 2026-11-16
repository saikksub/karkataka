import { Sequence } from "@karkataka/runtime";
import { render } from "solid-js/web";
import { expect, test } from "vitest";
import defaults from "../props.json";
import Quiz, { timeline } from "./Quiz.tsx";

// The quiz with its default props, as the whole video: 300 frames at 30 fps (the runtime's defaults outside karka).
const { codeAt, optionsAt, timerAt, revealAt } = timeline(defaults, 300, 30);
const root = document.createElement("div");
render(() => <Quiz {...defaults} />, root);
const at = (n: number) => {
  window.__setFrame!(n);
  return root.textContent!;
};

test("the timeline runs in order and ends with 3 seconds of answer", () => {
  expect(codeAt < optionsAt && optionsAt < timerAt && timerAt < revealAt).toBe(true);
  expect(revealAt).toBe(300 - 90);
});

test("each part appears at its frame", () => {
  expect(at(0)).toContain(defaults.question);
  expect(at(codeAt - 1)).not.toContain("#include");
  expect(at(codeAt)).toContain("#include <stdio.h>");
  expect(at(optionsAt - 1)).not.toContain(defaults.options[0]);
  for (const option of defaults.options) expect(at(optionsAt)).toContain(option);
  expect(at(timerAt - 1)).not.toContain("Answer");
});

test("the timer counts whole seconds down to the reveal, then the answer replaces it", () => {
  const seconds = (revealAt - timerAt) / 30;
  const number = (n: number) => {
    at(n);
    return root.querySelector("svg")?.nextElementSibling?.textContent ?? null;
  };
  expect(number(timerAt - 1)).toBe(null);
  expect(Number(number(timerAt))).toBe(Math.ceil(seconds));
  expect(number(revealAt - 1)).toBe("1");
  expect(number(revealAt)).toBe(null);
  expect(at(revealAt)).toContain(`Answer: B · ${defaults.options[defaults.answer]}`);
  expect(at(revealAt)).toContain(defaults.explanation);
});

test("lines of code keep their indentation", () => {
  at(codeAt + 100);
  const lines = Array.from(root.querySelectorAll<HTMLElement>("[style*='white-space']"), (l) => l.textContent);
  expect(lines).toEqual(defaults.code.split("\n").map((l) => l || " "));
});

test("audio: a tick each second of the countdown, then the ding at the reveal", () => {
  const { audio } = JSON.parse(window.__karkaReport!());
  const ticks = audio.filter((c: { src: string }) => c.src === "media/tick.wav").map((c: { from: number }) => c.from);
  expect(ticks).toEqual(Array.from({ length: Math.ceil((revealAt - timerAt) / 30) }, (_, s) => timerAt + s * 30));
  expect(audio.filter((c: { src: string }) => c.src === "media/ding.wav")).toEqual([{ src: "media/ding.wav", from: revealAt, to: null, skip: 0, volume: 1 }]);
});

test("the Studio schema describes exactly the props in props.json", async () => {
  const { schema } = await import("./Quiz.tsx");
  expect(Object.keys(schema).sort()).toEqual(Object.keys(defaults).sort());
  for (const [name, s] of Object.entries(schema)) {
    const value = (defaults as Record<string, unknown>)[name];
    const type = Array.isArray(value) ? "array" : typeof value === "number" ? (Number.isInteger(value) ? "integer" : "number") : typeof value;
    expect(s.type, name).toBe(type);
  }
});

test("as a clip, the quiz uses its props and fits its timeline to the clip's length", () => {
  const clip = document.createElement("div");
  // A 240-frame clip starting at frame 1000 of a longer video.
  const stop = render(
    () => (
      <Sequence from={1000} duration={240}>
        <Quiz question="Which is bigger?" />
      </Sequence>
    ),
    clip,
  );
  const reveal = timeline(defaults, 240, 30).revealAt;
  expect(reveal).toBe(240 - 90);
  // Too short for 3 s of answer: the countdown still gets 1 s.
  expect(timeline(defaults, 150, 30).revealAt).toBe(timerAt + 30);
  window.__setFrame!(1000);
  expect(clip.textContent).toContain("Which is bigger?");
  window.__setFrame!(1000 + reveal - 1);
  expect(clip.textContent).not.toContain("Answer");
  window.__setFrame!(1000 + reveal);
  expect(clip.textContent).toContain("Answer: B");
  stop();
});
