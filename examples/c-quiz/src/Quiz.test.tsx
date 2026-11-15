import { render } from "solid-js/web";
import { expect, test } from "vitest";
import defaults from "../props.json";
import { codeAt, optionsAt, Quiz, revealAt, timerAt } from "./Quiz.tsx";

// No --props or --frames here: the scene uses props.json and 300 frames at 30 fps.
const root = document.createElement("div");
render(() => <Quiz />, root);
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
