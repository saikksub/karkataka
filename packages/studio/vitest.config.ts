import { defineConfig } from "vitest/config";

// The plugin tests start real Vite servers and builds.
export default defineConfig({ test: { environment: "node", testTimeout: 30_000 } });
