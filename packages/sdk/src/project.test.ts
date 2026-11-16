import { expect, test } from "vitest";
import { durationOf, validate, type Project } from "./project.ts";

const base = (): Project => ({
  version: 1,
  width: 1920,
  height: 1080,
  fps: 30,
  tracks: [
    {
      id: "V1",
      clips: [
        { id: "a", type: "video", src: "a.mp4", start: 0, duration: 60, trim: 10 },
        { id: "t", type: "text", text: "Hi", start: 60, duration: 30, fadeIn: 5 },
      ],
    },
    { id: "A1", clips: [{ id: "m", type: "audio", src: "m.wav", start: 0, duration: 120, volume: 0.5 }] },
  ],
});
const messages = (p: unknown, exists?: (path: string) => boolean) => validate(p, exists).map((x) => `${x.path} ${x.message}`);

test("a good project has no problems, and its length is where the last clip ends", () => {
  expect(validate(base())).toEqual([]);
  expect(durationOf(base())).toBe(120);
  expect(durationOf({ ...base(), duration: 50 })).toBe(50);
  expect(durationOf({ ...base(), tracks: [] })).toBe(1);
});

test("project settings are checked", () => {
  expect(messages(null)).toEqual([" a project is a JSON object"]);
  expect(messages({ ...base(), version: 2, width: 1919, height: 0, fps: 0, duration: 1.5, background: 3, tracks: {} })).toEqual([
    "version must be 1",
    "width must be an even whole number, at least 72",
    "height must be an even whole number, at least 2",
    "fps must be a whole number from 1 to 240",
    "duration must be a whole number, at least 1",
    "background must be a CSS color",
    "tracks must be a list of tracks",
  ]);
});

test("clips are checked field by field, with the clip's id in the path", () => {
  const p = base();
  p.tracks[0].clips = [
    { id: "a", type: "video", src: "", start: -1, duration: 0, trim: -2, volume: -1, fit: "zoom" as "cover", opacity: 2, fadeIn: 99, box: { x: 0, y: 0, width: -5, height: 1 } },
  ];
  expect(messages(p)).toEqual([
    'tracks[0].clips[0] ("a").start must be a whole number of frames, at least 0',
    'tracks[0].clips[0] ("a").duration must be a whole number of frames, at least 1',
    'tracks[0].clips[0] ("a").opacity must be from 0 to 1',
    'tracks[0].clips[0] ("a").fadeIn must be a whole number of frames, at most the clip\'s duration',
    'tracks[0].clips[0] ("a").box must be {x, y, width, height} in px',
    'tracks[0].clips[0] ("a").src must be the path of a video file',
    'tracks[0].clips[0] ("a").trim must be a whole number of frames, at least 0',
    'tracks[0].clips[0] ("a").volume must be 0 or more',
    'tracks[0].clips[0] ("a").fit must be cover, contain or fill',
  ]);
});

test("unknown types and fields, duplicate ids, and other clip kinds", () => {
  const p = base();
  p.tracks[0].clips = [
    { id: "x", type: "sticker" as "text", text: "", start: 0, duration: 1 },
    { id: "t", type: "text", text: 5 as unknown as string, style: "big" as never, start: 1, duration: 1, size: 3 } as never,
    { id: "c", type: "color", color: "", start: 2, duration: 1 },
    { id: "k", type: "component", component: "Q.tsx", props: [] as never, start: 3, duration: 1 },
    { id: "i", type: "image", src: "i.png", start: 4, duration: 1 },
  ];
  p.tracks[1].id = "V1";
  expect(messages(p)).toEqual([
    'tracks[0].clips[0] ("x").type must be one of video, image, audio, text, color, component',
    'tracks[0].clips[1] ("t").size is not a text clip field (fields: id, type, text, start, duration, style, opacity, fadeIn, fadeOut, box)',
    'tracks[0].clips[1] ("t").text must be a string',
    'tracks[0].clips[1] ("t").style must be an object',
    'tracks[0].clips[2] ("c").color must be a CSS color',
    'tracks[0].clips[3] ("k").props must be an object',
    'tracks[1].id "V1" is used twice; ids must be unique',
  ]);
});

test("clips on one track must not overlap", () => {
  const p = base();
  p.tracks[0].clips[1].start = 50;
  expect(messages(p)).toEqual(['tracks[0] clips "a" (frames 0–59) and "t" (from 50) overlap; move one or put it on another track']);
});

test("with a file check, missing media and components are reported; URLs are not checked", () => {
  const p = base();
  p.tracks[0].clips[0] = { id: "a", type: "video", src: "https://x/a.mp4", start: 0, duration: 60 };
  expect(messages(p, (path) => path !== "m.wav")).toEqual(['tracks[1].clips[0] ("m").src file not found: m.wav']);
});
