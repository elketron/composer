// The composer app sdk: theme, components and the server client.
// Apps import it as `$sdk` (a Vite alias to ~/.composer/templates/sdk), so fixes here
// reach every app on its next build.

import './theme.css';

export * from './client.svelte';
export { default as AppShell } from './components/AppShell.svelte';
export { default as Badge } from './components/Badge.svelte';
export { default as Button } from './components/Button.svelte';
export { default as Kbd } from './components/Kbd.svelte';
