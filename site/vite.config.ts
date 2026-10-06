// SPDX-License-Identifier: Apache-2.0
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// The public site. Its build output is uploaded to static hosting, and its
// figures come from `realorrug-serve`'s published documents.
//
// It was split from Radar's own interface for this reason: Radar's server takes 3.2s per
// request on its store-backed routes, on two cores shared with Cortex and with
// the recorder — see design 0008 §2. This site is the link the bot posts, so it
// is the one surface that can be hit by everybody at once, and it must not be
// able to take the recorder down.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    minify: "esbuild",
    sourcemap: false,
  },
  server: {
    // Not 5173, and not `web`'s 5273. Three dev servers can live in this
    // checkout tree and reading the wrong application's page while believing it
    // is yours is a real way to waste an hour.
    port: 5373,
    strictPort: true,
    // The public endpoints, when a local `realorrug-serve` is running on its
    // default port. Legacy pages can fall back to dated committed fixtures;
    // Library pages disclose unavailable service and never invent case data.
    proxy: {
      "/v1/public": "http://127.0.0.1:8090",
      "/v1/library": "http://127.0.0.1:8090",
      "/v1/cases": "http://127.0.0.1:8090",
      "/v1/investigations": "http://127.0.0.1:8090",
      "/auth": "http://127.0.0.1:8090",
    },
  },
});
