import assert from "node:assert/strict";
import { test } from "vitest";
import { interpolate, spring } from "./math.ts";

const near = (a: number, b: number, eps = 1e-6) => assert.ok(Math.abs(a - b) < eps, `${a} is not near ${b}`);

test("interpolate", () => {
  near(interpolate(5, [0, 10], [0, 100]), 50);
  near(interpolate(-5, [0, 10], [0, 100]), 0);
  near(interpolate(15, [0, 10], [0, 100]), 100);
  near(interpolate(15, [0, 10], [0, 100], { clamp: false }), 150);
  near(interpolate(-5, [0, 10], [0, 100], { clamp: false }), -50);
  // Several segments, and easing.
  near(interpolate(15, [0, 10, 20], [0, 1, 0]), 0.5);
  near(interpolate(20, [0, 10, 20], [0, 1, 0]), 0);
  near(interpolate(5, [0, 10], [0, 1], { easing: (t) => t * t }), 0.25);
  assert.throws(() => interpolate(0, [0], [0]));
  assert.throws(() => interpolate(0, [0, 1], [0, 1, 2]));
  assert.throws(() => interpolate(0, [1, 1], [0, 1]));
});

test("spring", () => {
  for (const damping of [10, 20, 40]) {
    // Under-, critically and over-damped (stiffness 100, mass 1: critical at 20).
    near(spring({ frame: 0, damping }), 0);
    near(spring({ frame: -5, damping, from: 3 }), 3);
    near(spring({ frame: 600, damping, from: 2, to: 7 }), 7, 1e-3);
    // Starts at rest: the first frame barely moves.
    assert.ok(spring({ frame: 1, damping }) < 0.15);
    // Continuous between frames.
    near(spring({ frame: 10, damping }), spring({ frame: 10.0001, damping }), 1e-3);
  }
  const peak = (damping: number) => Math.max(...Array.from({ length: 300 }, (_, f) => spring({ frame: f, damping })));
  assert.ok(peak(10) > 1.1, "the default spring overshoots");
  assert.ok(peak(20) <= 1 && peak(40) <= 1, "critical and overdamped springs do not");
});
