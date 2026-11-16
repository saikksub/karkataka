import { studio } from "@karkataka/studio";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// `pnpm media` once, then `pnpm dev` serves the Studio for edit.json at /studio.
export default defineConfig({ plugins: [solid(), studio({ project: "edit.json" })] });
