import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { viteSingleFile } from "vite-plugin-singlefile";

// The engine loads one HTML string, so everything is inlined into dist/index.html.
export default defineConfig({ plugins: [solid(), viteSingleFile()] });
