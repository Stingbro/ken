/// <reference types="svelte" />
/// <reference types="vite/client" />

/** App version baked in from package.json by vite.config.ts's `define`. */
declare const __APP_VERSION__: string;

/** `?raw` imports (e.g. WHATS_NEW.md) resolve to the file's text. */
declare module "*.md?raw" {
  const content: string;
  export default content;
}
