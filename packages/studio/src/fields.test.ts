import { expect, test } from "vitest";
import { kindOf, propNames, toNumber } from "./fields.ts";

test("the schema picks the editor", () => {
  expect(kindOf({ type: "string" }, "x")).toBe("text");
  expect(kindOf({ type: "string", multiline: true }, "x")).toBe("multiline");
  expect(kindOf({ type: "integer" }, 1)).toBe("number");
  expect(kindOf({ type: "number" }, 1.5)).toBe("number");
  expect(kindOf({ type: "boolean" }, true)).toBe("boolean");
  expect(kindOf({ type: "string", enum: ["a", "b"] }, "a")).toBe("select");
  expect(kindOf({ type: "array", items: { type: "string" } }, [])).toBe("list");
  expect(kindOf({ type: "array", items: { type: "number" } }, [1])).toBe("json");
  expect(kindOf({ type: "object" }, {})).toBe("json");
});

test("without a schema, the value picks the editor", () => {
  expect(kindOf(undefined, "one line")).toBe("text");
  expect(kindOf(undefined, "two\nlines")).toBe("multiline");
  expect(kindOf(undefined, 3)).toBe("number");
  expect(kindOf(undefined, false)).toBe("boolean");
  expect(kindOf(undefined, ["a", "b"])).toBe("list");
  expect(kindOf(undefined, [1, 2])).toBe("json");
  expect(kindOf(undefined, { a: 1 })).toBe("json");
  expect(kindOf(undefined, null)).toBe("json");
});

test("numbers follow minimum, maximum and integer", () => {
  const answer = { type: "integer" as const, minimum: 0, maximum: 3 };
  expect(toNumber(answer, "2")).toBe(2);
  expect(toNumber(answer, "9")).toBe(3);
  expect(toNumber(answer, "-1")).toBe(0);
  expect(toNumber(answer, "1.6")).toBe(2);
  expect(toNumber({ type: "number" }, "1.6")).toBe(1.6);
  expect(toNumber(undefined, "")).toBeUndefined();
  expect(toNumber(undefined, "abc")).toBeUndefined();
});

test("schema props come first, in schema order, then the other defaults", () => {
  expect(propNames({ a: 1, b: 2, c: 3 }, { c: {}, a: {} })).toEqual(["c", "a", "b"]);
  expect(propNames({ a: 1 })).toEqual(["a"]);
});
