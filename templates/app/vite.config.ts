import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// The shared sdk lives in ~/.composer/templates/sdk (this file is in ~/.composer/sites/<site>/ui).
const sdk = fileURLToPath(new URL('../../../templates/sdk', import.meta.url));

export default defineConfig({
  // Served by the composer server at /__SITE__/.
  base: './',
  plugins: [svelte()],
  resolve: {
    alias: { $sdk: sdk },
    // The sdk's components must use this app's copy of svelte.
    dedupe: ['svelte'],
  },
  server: { fs: { allow: ['.', sdk] } },
});
