// Chromium render + interaction QA. No build or server required.
import { mkdir, readdir } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = process.env.OUT || '/tmp/opencode/composer-next';
const module = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { chromium } = module.default ?? module;
const browser = await chromium.launch({ executablePath: process.env.CHROME || undefined });
const screens = (await readdir(path.join(root, 'screens'))).filter(name => name.endsWith('.html')).sort();
const desktopScreens = screens.filter(name => !name.includes('-phone-'));
const phoneScreens = screens.filter(name => name.includes('-phone-') && !name.includes('landscape'));
const url = rel => pathToFileURL(path.join(root, rel)).href;
await mkdir(out, { recursive: true });
try {
  for (const [width, height] of [[2000, 1246], [1440, 1000], [820, 900]]) {
    const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1 });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    page.on('requestfailed', request => errors.push(`${request.url()}: ${request.failure()?.errorText}`));
    for (const rel of [...desktopScreens.map(name => `screens/${name}`), 'index.html']) {
      await page.goto(url(rel), { waitUntil: 'load' });
      await Promise.all(page.frames().map(frame => frame.evaluate(() => document.fonts.ready)));
      const metrics = await page.evaluate(() => {
        const rect = selector => {
          const r = document.querySelector(selector)?.getBoundingClientRect();
          return r ? { x: r.x, y: r.y, right: r.right, bottom: r.bottom, width: r.width, height: r.height } : null;
        };
        return {
          overflow: document.documentElement.scrollWidth > innerWidth,
          footer: rect('.statusbar'), stage: rect('.stage'), chat: rect('.chat'),
          overlay: rect('.overlay'), composer: rect('.composer'),
          font: document.fonts.check('12px "JetBrains Mono"'),
          optionsOverflow: [...document.querySelectorAll('.option')].some(element => element.scrollWidth > element.clientWidth + 2),
        };
      });
      assert(!metrics.overflow, `Horizontal page overflow: ${rel} @${width}`);
      assert(metrics.font, 'Local font did not load');
      if (metrics.footer) assert(metrics.footer.bottom <= height && metrics.footer.y > 0, 'Footer offscreen');
      if (metrics.footer && metrics.composer) {
        assert(metrics.composer.bottom <= metrics.footer.y, 'Composer offscreen');
        assert(metrics.stage.width > 0, 'Missing stage');
        assert(!metrics.optionsOverflow, `Overlay row overflow: ${rel} @${width}`);
        if (metrics.overlay) {
          // Dead centre of the window between the bars, inside the area, never under a bar.
          const middle = 44 + (height - 70) / 2;
          assert(Math.abs(metrics.overlay.x + metrics.overlay.width / 2 - width / 2) <= 1, `Overlay not centred across: ${rel} @${width}`);
          assert(Math.abs(metrics.overlay.y + metrics.overlay.height / 2 - middle) <= 1, `Overlay not centred down: ${rel} @${width}`);
          assert(metrics.overlay.x >= 16 && metrics.overlay.right <= width - 16, `Overlay leaves the window: ${rel} @${width}`);
          assert(metrics.overlay.y >= 44 && metrics.overlay.bottom <= height - 26, `Overlay covers a bar: ${rel} @${width}`);
        }
      }
      const name = rel.replace('screens/', '').replace('.html', '');
      await page.screenshot({ path: path.join(out, `${name}-${width}.png`), fullPage: true });
      console.log(`ok render ${name} @${width}×${height}`);
    }
    assert.equal(errors.length, 0, errors.join('\n'));
    await page.close();
  }

  for (const [width, height] of [[390, 844], [360, 740], [820, 900]]) {
    const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1, hasTouch: true });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    page.on('requestfailed', request => errors.push(`${request.url()}: ${request.failure()?.errorText}`));
    for (const name of phoneScreens) {
      await page.goto(url(`screens/${name}`), { waitUntil: 'load' });
      await page.evaluate(() => document.fonts.ready);
      assert(await page.evaluate(() => document.fonts.check('15px Inter')), 'Phone Inter font did not load');
      assert((await page.locator('.phone-shell').evaluate(element => getComputedStyle(element).fontFamily)).includes('Inter'), 'Phone lost its current UI font');
      if (await page.locator('.p-transcript').count()) {
        assert.equal(await page.locator('.p-transcript > :first-child .p-approval').count(), 1, 'Phone is not newest-first');
        assert.equal(await page.locator('.p-transcript').evaluate(element => element.scrollTop), 0);
        assert.deepEqual(await page.locator('.p-you').first().evaluate(element => {
          const style = getComputedStyle(element);
          return [style.borderTopLeftRadius, style.borderBottomRightRadius];
        }), ['5px', '3px'], 'User message separation lost');
      }
      const metrics = await page.evaluate(() => {
        const bounds = element => {
          const r = element.getBoundingClientRect();
          return { x: r.x, y: r.y, bottom: r.bottom, right: r.right, width: r.width, height: r.height };
        };
        const find = selector => {
          const element = document.querySelector(selector);
          return element && element.getClientRects().length ? bounds(element) : null;
        };
        return {
          overflow: document.documentElement.scrollWidth > innerWidth,
          pageScroll: document.documentElement.scrollHeight > innerHeight,
          shell: find('.phone-shell'), composer: find('.p-composer'), sheet: find('.p-sheet'),
          gesture: find('.p-app > .p-gesture'),
          badTargets: [...document.querySelectorAll('.phone-shell a, .phone-shell button')]
            .filter(element => !element.closest('[inert]') && element.getClientRects().length)
            .map(element => ({ text: element.textContent.trim(), ...bounds(element) }))
            .filter(r => r.width < 44 || r.height < 44),
          contentOverflow: [...document.querySelectorAll('.p-content, .p-sheet')].some(element => element.scrollWidth > element.clientWidth + 1),
          approval: find('.p-approval-actions'), content: find('.p-content'),
        };
      });
      assert(!metrics.overflow && !metrics.pageScroll, `Phone page overflow: ${name} @${width}`);
      assert(!metrics.contentOverflow, `Phone content overflow: ${name} @${width}`);
      assert.equal(metrics.badTargets.length, 0, `Small touch targets: ${JSON.stringify(metrics.badTargets)}`);
      assert(metrics.shell.width <= 430 && metrics.shell.bottom <= height, 'Phone canvas bounds');
      assert(metrics.gesture.bottom <= height, 'System navigation offscreen');
      if (metrics.composer) assert(metrics.composer.bottom <= metrics.gesture.y, 'Composer offscreen');
      if (metrics.sheet) {
        assert(metrics.sheet.y > 28 && metrics.sheet.bottom <= height, 'Sheet bounds');
        assert.equal(await page.locator('.p-app').getAttribute('inert'), '');
        assert.equal(await page.locator(':focus').getAttribute('aria-label'), 'Close site sheet');
      } else if (metrics.approval) {
        assert(metrics.approval.bottom <= metrics.content.bottom + 1, 'Approval controls hidden by composer');
        assert(metrics.approval.y >= metrics.content.y, 'Approval controls not reachable in initial view');
      }
      assert.equal(await page.getByRole('button', { name: 'Drive', exact: true }).count(), 0);
      await page.screenshot({ path: path.join(out, `${name.replace('.html', '')}-${width}.png`), fullPage: true });
      console.log(`ok phone render ${name} @${width}×${height}, 44px targets, composer, view-only`);
    }
    assert.equal(errors.length, 0, errors.join('\n'));
    await page.close();
  }

  for (const [width, height] of [[844, 390], [740, 360], [932, 430]]) {
    const page = await browser.newPage({ viewport: { width, height }, hasTouch: true });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    const inspect = async () => page.evaluate(() => {
      const rect = selector => {
        const element = document.querySelector(selector);
        if (!element?.getClientRects().length) return null;
        const r = element.getBoundingClientRect();
        return { x: r.x, y: r.y, right: r.right, bottom: r.bottom, width: r.width, height: r.height };
      };
      return {
        shell: rect('.phone-shell'), chat: rect('.p-transcript'), composer: rect('.p-composer'),
        viewer: rect('.p-live-view'), controls: rect('.p-approval-actions'),
        overflow: document.documentElement.scrollWidth > innerWidth || document.documentElement.scrollHeight > innerHeight,
        contentOverflow: [...document.querySelectorAll('.p-content')].some(element => element.scrollWidth > element.clientWidth + 1),
        smallTargets: [...document.querySelectorAll('a, button')].filter(element => {
          if (!element.getClientRects().length || element.closest('[inert]')) return false;
          const r = element.getBoundingClientRect();
          return r.width < 44 || r.height < 44;
        }).length,
      };
    });
    const check = async () => {
      const m = await inspect();
      assert(!m.overflow && !m.contentOverflow, 'Landscape overflow');
      assert.equal(m.smallTargets, 0, 'Landscape target below 44px');
      assert(m.shell.width >= width - 2 && m.shell.bottom <= height, 'Landscape retained portrait cap');
      if (m.chat) {
        assert(m.chat.height >= 130 && m.composer.bottom <= height, 'Chat or composer squeezed out');
        assert(m.controls.y >= m.chat.y && m.controls.bottom <= m.chat.bottom + 1, 'Approval controls offscreen');
        if (m.viewer) {
          assert(m.chat.right <= m.viewer.x + 1, 'Chat must be left of live view');
          assert(m.composer.right <= m.viewer.x + 1, 'Composer not under left chat');
        } else assert(m.chat.width >= m.shell.width - 60 && await page.getByRole('button', { name: 'Show live view' }).isVisible(), 'Folded live view is not a rail beside a full chat');
      } else {
        assert(!m.composer && m.viewer.width >= m.shell.width - 2, 'Focus mode did not expand the viewer');
        assert(await page.getByRole('button', { name: 'Show chat', exact: true }).isVisible());
      }
      assert.equal(await page.getByRole('button', { name: 'Drive', exact: true }).count(), 0);
    };
    for (const name of ['05-phone-chat', '06-phone-live', '08-phone-landscape', '09-phone-focus']) {
      await page.goto(url(`screens/${name}.html`));
      await page.evaluate(() => document.fonts.ready);
      await check();
      await page.screenshot({ path: path.join(out, `${name}-${width}.png`) });
      console.log(`ok landscape ${name} @${width}×${height}`);
    }
    await page.goto(url('screens/08-phone-landscape.html'));
    await page.evaluate(() => document.fonts.ready);
    const savedScroll = await page.locator('.p-transcript').evaluate(element => { element.scrollTop = 40; return element.scrollTop; });
    await page.getByRole('button', { name: 'Hide chat', exact: true }).tap();
    await check();
    assert.equal(await page.locator('.p-message').count(), 7, 'Hiding chat removed conversation state');
    await page.screenshot({ path: path.join(out, `08-phone-landscape-focus-${width}.png`) });
    await page.getByRole('button', { name: 'Show chat', exact: true }).tap();
    await page.evaluate(() => new Promise(requestAnimationFrame));
    assert.equal(await page.locator('.p-transcript').evaluate(element => element.scrollTop), savedScroll);
    await page.getByRole('button', { name: 'Hide live view' }).tap();
    await page.evaluate(() => new Promise(requestAnimationFrame));
    await check();
    await page.screenshot({ path: path.join(out, `08-phone-landscape-collapsed-${width}.png`) });
    await page.getByRole('button', { name: 'Show live view' }).tap();
    await page.getByRole('button', { name: 'Hide chat', exact: true }).tap();
    await page.getByRole('button', { name: 'Hide live view' }).tap();
    await page.evaluate(() => new Promise(requestAnimationFrame));
    await check();
    assert(await page.locator('.p-composer').isVisible(), 'Collapse did not restore hidden chat');
    assert.equal(errors.length, 0, errors.join('\n'));
    console.log(`ok landscape focus/restore scroll, collapse/expand and hidden-chat recovery @${width}`);
    await page.close();
  }

  const phone = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true });
  await phone.goto(url('screens/04-phone-inbox.html'));
  await phone.locator('a.p-session').tap();
  await phone.waitForURL('**/05-phone-chat.html');
  await phone.getByRole('link', { name: 'Expand live view' }).tap();
  await phone.waitForURL('**/06-phone-live.html');
  assert(await phone.locator('.p-live-view').isVisible());
  const portraitScroll = await phone.locator('.p-transcript').evaluate(element => { element.scrollTop = 30; return element.scrollTop; });
  await phone.getByRole('button', { name: 'Hide chat', exact: true }).tap();
  assert(!(await phone.locator('.p-composer').isVisible()));
  const focusedHeight = await phone.locator('.p-live-view').evaluate(element => element.clientHeight);
  assert(focusedHeight > 208, 'Portrait focus view did not grow');
  await phone.getByRole('button', { name: 'Show chat', exact: true }).tap();
  await phone.evaluate(() => new Promise(requestAnimationFrame));
  assert.equal(await phone.locator('.p-transcript').evaluate(element => element.scrollTop), portraitScroll);
  await phone.getByRole('button', { name: 'Hide chat', exact: true }).tap();
  await phone.getByRole('link', { name: 'Collapse live view' }).tap();
  await phone.waitForURL('**/05-phone-chat.html');
  await phone.getByRole('link', { name: 'Change site' }).tap();
  await phone.waitForURL('**/07-phone-site.html');
  await phone.keyboard.press('Tab');
  assert.equal(await phone.locator(':focus').getAttribute('aria-label'), '3d, current site');
  await phone.keyboard.press('Tab');
  assert.equal(await phone.locator(':focus').getAttribute('aria-label'), 'Close site sheet');
  await phone.keyboard.press('Escape');
  await phone.waitForURL('**/05-phone-chat.html');
  await phone.getByRole('link', { name: 'Change site' }).tap();
  await phone.getByRole('link', { name: 'Close site sheet' }).tap();
  await phone.waitForURL('**/05-phone-chat.html');
  await phone.getByRole('link', { name: 'Back to inbox' }).tap();
  await phone.waitForURL('**/04-phone-inbox.html');
  console.log('ok phone navigation, hide/show chat with scroll restore, collapse recovery, site sheet and focus');
  await phone.close();

  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  await page.goto(url('screens/01-workspace.html'));
  await page.getByRole('link', { name: /^Sessions/ }).click();
  assert.equal(await page.locator('.option').count(), 4);
  await page.getByRole('combobox').fill('review');
  assert.equal(await page.locator('.option:visible').count(), 1);
  assert.equal(await page.locator('[data-count]').textContent(), '1 session');
  await page.getByRole('combobox').fill('no-such-session');
  assert(await page.locator('.empty').isVisible());
  await page.getByRole('combobox').fill('');
  await page.keyboard.press('ArrowDown');
  assert.equal(await page.locator('.selected').getAttribute('id'), 'session-review');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('Enter');
  await page.waitForURL('**/01-workspace.html');
  await page.getByRole('link', { name: /^Commands/ }).click();
  assert.equal(await page.locator('.selected').getAttribute('id'), 'command-split');
  await page.getByRole('combobox').fill('site');
  assert.equal(await page.locator('.command-group:visible').count(), 2);
  await page.getByRole('combobox').fill('switch session');
  await page.keyboard.press('Enter');
  await page.waitForURL('**/02-sessions.html');
  await page.keyboard.press('Escape');
  await page.waitForURL('**/01-workspace.html');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForURL('**/03-commands.html');
  console.log('ok overlay links, filtering, empty state, arrow keys, Enter, Escape and palette shortcut');
  await page.goto(url('screens/01-workspace.html'));
  await page.locator('.titlebar .crumb').click();
  await page.waitForURL('**/11-desktop-home.html');
  assert.equal(await page.locator('.chat').count(), 0, 'Home is outside the session');
  await page.getByRole('link', { name: /review-0014/ }).first().click();
  await page.waitForURL('**/10-desktop-review.html');
  assert.equal(await page.locator('.chat').count(), 1);
  assert.equal(await page.locator('.stage .hunk').count(), 1, 'Review lives on the stage');
  await page.locator('.titlebar .crumb').click();
  await page.waitForURL('**/11-desktop-home.html');
  await page.getByRole('link', { name: /^projects/ }).click();
  await page.waitForURL('**/12-desktop-projects.html');
  await page.keyboard.press('Escape');
  await page.waitForURL('**/01-workspace.html');
  console.log('ok workspace home, projects, review state and the way back');
  await page.close();
} finally {
  await browser.close();
}
console.log(`Screenshots: ${out}`);
