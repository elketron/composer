import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// The shared sdk: the home's templates/sdk, which composer passes to recipes as COMPOSER_SDK (in
// the sandbox it is also at <workspace>/templates/sdk, which the relative fallback finds).
const sdk =
  process.env.COMPOSER_SDK ?? fileURLToPath(new URL('../../../templates/sdk', import.meta.url));

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
