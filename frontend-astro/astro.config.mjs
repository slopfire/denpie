// Static output only: Astro owns the HTML document shell. There is NO SSR
// adapter, NO Astro Actions, and NO Node backend — the Axum server in `src/`
// remains the sole server and API surface (`POST /auth/login`, `GET /auth/me`,
// `POST /auth/logout`, ...). `astro dev` (used by `just dev`) is a local Vite
// HMR process only; it proxies API/session routes to Axum and is not part of
// `astro build` or production.
import { readFileSync } from "node:fs";
import { defineConfig } from "astro/config";
import react from "@astrojs/react";
import tailwindcss from "@tailwindcss/vite";

// Checked-in lab card fixtures, inlined at build time via Vite `define` so
// the lab page renders the exact pack under `lab/cases/cards/` without a
// network call or a copy drifting from the source of truth.
const labCardFixtures = readFileSync(
  new URL("../lab/cases/cards/repeatable-states.json", import.meta.url),
  "utf8",
);

const axumOrigin = (() => {
  const bind = process.env.DENPIE_BIND_ADDR || "127.0.0.1:3017";
  if (/^https?:\/\//.test(bind)) {
    return bind;
  }
  return `http://${bind}`;
})();

const axumProxy = {
  "/api": axumOrigin,
  "/auth": axumOrigin,
  "/app": axumOrigin,
  "/admin": axumOrigin,
  "/static": axumOrigin,
};

export default defineConfig({
  output: "static",
  site: "http://localhost:3027",
  integrations: [react()],
  server: {
    port: 4321,
  },
  vite: {
    plugins: [tailwindcss()],
    define: {
      __LAB_CARD_FIXTURES__: JSON.stringify(labCardFixtures),
    },
    server: {
      strictPort: true,
      proxy: axumProxy,
    },
  },
});
