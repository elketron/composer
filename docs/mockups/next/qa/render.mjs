// Render the next screens headlessly and fail on overflow.
//
//   PLAYWRIGHT_MODULE=/usr/lib/node_modules/playwright/index.js \
//   CHROME=/usr/bin/chromium \
//   node docs/mockups/next/qa/render.mjs
//
// Screenshots land in /tmp/composer-next/. The static source check is
// qa/check.py; this one is about what actually rendered.

import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const out = process.env.OUT || '/tmp/composer-next';
const widths = [1440, 1100, 820];
const screens = [
  'screens/01-desktop-live.html',
  'screens/02-desktop-mockups.html',
  'screens/03-desktop-project.html',
  'screens/04-phone-session.html',
  'screens/05-tui-workflow.html',
  'screens/06-desktop-gpui.html',
  'screens/07-desktop-gpui-palette.html',
  'screens/08-phone-touch.html',
  'screens/09-phone-workflow-sheet.html',
  'index.html',
];

const modulePath = process.env.PLAYWRIGHT_MODULE || 'playwright';
let playwright;
try {
  playwright = await import(modulePath);
} catch (error) {
  console.error(`Cannot load Playwright from ${modulePath}`);
  console.error('Set PLAYWRIGHT_MODULE to the module or install playwright.');
  process.exit(2);
}
const { chromium } = playwright.default ?? playwright;
const executablePath = process.env.CHROME || undefined;

await mkdir(out, { recursive: true });
const browser = await chromium.launch(executablePath ? { executablePath } : {});
let failed = false;

for (const width of widths) {
  const page = await browser.newPage({ viewport: { width, height: 1000 } });
  for (const rel of screens) {
    const url = `file://${path.join(root, rel)}`;
    await page.goto(url, { waitUntil: 'load' });
    if (rel === 'index.html') {
      // Chromium paints iframes only as they enter the viewport.
      await page.evaluate(async () => {
        for (let y = 0; y < document.body.scrollHeight; y += 500) {
          window.scrollTo(0, y);
          await new Promise((resolve) => setTimeout(resolve, 100));
        }
        window.scrollTo(0, 0);
      });
    }
    await page.waitForTimeout(600);
    const result = await page.evaluate(() => {
      const doc = document.documentElement;
      const frames = [...document.querySelectorAll('.fit')].map((box) => {
        const frame = box.firstElementChild;
        const r = frame ? frame.getBoundingClientRect() : null;
        return {
          height: Math.round(box.getBoundingClientRect().height),
          scaledWidth: r ? Math.round(r.width) : 0,
          naturalWidth: frame ? frame.offsetWidth : 0,
        };
      });
      return {
        scrollWidth: doc.scrollWidth,
        viewport: window.innerWidth,
        frames,
      };
    });
    const name = rel.replace(/[/]/g, '-').replace('.html', '');
    const file = path.join(out, `${name}-${width}.png`);
    await page.screenshot({ path: file, fullPage: true });

    const overflow = result.scrollWidth > result.viewport + 2;
    const badFrame = result.frames.some((frame) => frame.height < 1 || frame.scaledWidth < 1);
    if (overflow || badFrame) {
      failed = true;
      console.log(`FAIL ${name} @${width}: scrollWidth ${result.scrollWidth}/${result.viewport}` + (badFrame ? ` frames ${JSON.stringify(result.frames)}` : ''));
    } else {
      console.log(`ok   ${name} @${width} -> ${file}`);
    }
  }
  await page.close();
}

await browser.close();
if (failed) process.exit(1);
