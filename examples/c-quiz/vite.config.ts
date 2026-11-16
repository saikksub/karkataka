import { studio } from "@karkataka/studio";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { viteSingleFile } from "vite-plugin-singlefile";

// The engine loads one HTML string, so everything is inlined into dist/index.html.
// studio(): `pnpm dev` also serves the Studio at /studio. It renders with $KARKA (default: karka on PATH).
export default defineConfig({ plugins: [solid(), viteSingleFile(), studio()] });
