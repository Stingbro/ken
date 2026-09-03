import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { mkdirSync, writeFileSync } from "node:fs";

// The PDF harness saves form fills here so they can be inspected on disk.
const OUT = new URL("./out/", import.meta.url).pathname;

function saveEndpoint(): Plugin {
  return {
    name: "harness-save",
    configureServer(server) {
      server.middlewares.use("/__save", (req, res) => {
        if (req.method !== "POST") {
          res.statusCode = 405;
          return res.end();
        }
        const chunks: Buffer[] = [];
        req.on("data", (c: Buffer) => chunks.push(c));
        req.on("end", () => {
          mkdirSync(OUT, { recursive: true });
          writeFileSync(`${OUT}saved.pdf`, Buffer.concat(chunks));
          res.statusCode = 204;
          res.end();
        });
      });
    },
  };
}

// Dev-only: serves harness/pptx.html and harness/pdf.html on :1421 with
// harness/decks as static root.
export default defineConfig({
  root: new URL(".", import.meta.url).pathname,
  plugins: [svelte(), saveEndpoint()],
  publicDir: new URL("./decks", import.meta.url).pathname,
  server: { port: 1421, strictPort: true },
});
