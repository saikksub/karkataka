import { studio } from "@karkataka/studio";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// `pnpm dev` serves the Studio for quiz.json at /studio. Its render button runs $KARKA (default: karka on PATH).
export default defineConfig({ plugins: [solid(), studio({ project: "quiz.json" })] });
