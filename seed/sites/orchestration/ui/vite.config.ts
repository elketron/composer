import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// The shared sdk: the home's templates/sdk, which composer passes to recipes as COMPOSER_SDK. The
// fallback is where it was before workspaces (this file in ~/.composer/sites/<site>/ui).
const sdk =
  process.env.COMPOSER_SDK ?? fileURLToPath(new URL('../../../templates/sdk', import.meta.url));

export default defineConfig({
  // Served by the composer server at /orchestration/.
  base: './',
  plugins: [svelte()],
  resolve: {
    alias: { $sdk: sdk },
    // The sdk's components must use this app's copy of svelte.
    dedupe: ['svelte'],
  },
  server: { fs: { allow: ['.', sdk] } },
});
