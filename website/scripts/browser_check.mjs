import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { chromium, firefox, webkit } from 'playwright';
import AxeBuilder from '@axe-core/playwright';

const url = process.env.QA_URL || 'http://127.0.0.1:8798/volparossa-bank/';
const artifacts = new URL('../qa/browser/', import.meta.url).pathname;
await mkdir(artifacts, {recursive: true});
const engines = {chromium, firefox, webkit};
for (const engine of (process.env.QA_ENGINES || 'chromium').split(',')) {
  const options = engine === 'chromium' && process.env.CHROME_PATH ? {executablePath: process.env.CHROME_PATH} : {};
  const browser = await engines[engine].launch(options);
  try {
    for (const width of [1440, 768, 390, 320]) {
      const context = await browser.newContext({viewport: {width, height: 900}, reducedMotion: 'reduce'});
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror', e => errors.push(e.message));
      page.on('response', r => { if (r.status() >= 400) errors.push(r.url()); });
      assert.equal((await page.goto(url)).status(), 200);
      await page.evaluate(async () => {
        for (const img of document.images) img.loading = 'eager';
        await Promise.all(Array.from(document.images, img => img.decode()));
        await document.fonts.ready;
      });
      async function checkOverflow(mode) {
        const scrollWidth = await page.evaluate(() => document.documentElement.scrollWidth);
        assert(scrollWidth <= width + 1, engine + ' ' + mode + ' overflows at ' + width + ': ' + scrollWidth);
      }
      await checkOverflow('default');
      if ([1440,390].includes(width)) await page.screenshot({path: artifacts + engine + '-' + width + '.png', fullPage: true});
      await page.keyboard.press('Tab');
      assert.equal(await page.locator(':focus').textContent(), 'Skip to content');
      await page.keyboard.press('Enter');
      assert.equal(await page.locator(':focus').getAttribute('id'), 'main');
      const menu = page.locator('.mobile-nav');
      const mobile = await menu.isVisible();
      const nav = page.locator(mobile ? '.mobile-nav' : '.desktop-nav');
      if (mobile) await menu.locator('summary').click();
      const toggle = nav.locator('.applications-toggle');
      if (mobile) await toggle.click();
      else await nav.locator('.applications-menu').hover();
      assert(await nav.getByRole('link', {name: 'Bank', exact: true}).isVisible());
      await toggle.focus();
      await page.keyboard.press('Escape');
      assert.equal(await toggle.getAttribute('aria-expanded'), 'false');
      if (mobile) await page.keyboard.press('Escape');
      await page.getByRole('link', {name: 'Explore the method'}).click();
      assert.equal(new URL(page.url()).hash, '#method');
      const summary = page.locator('.calculation-details > summary');
      await summary.focus();
      await page.keyboard.press('Enter');
      assert(await page.getByRole('table', {name: 'Worked example with fictitious companies'}).isVisible());
      await checkOverflow('calculation');
      if (engine === 'chromium' && [320,1440].includes(width)) {
        const audit = await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa','wcag21a','wcag21aa']).analyze();
        assert.deepEqual(audit.violations.map(v => [v.id, v.nodes.map(n => n.target)]), []);
      }
      if ([320,768,1440].includes(width)) {
        await page.addStyleTag({content: 'html{font-size:200%}'});
        await checkOverflow('200% text');
      }
      await page.goto(url + '#payment-test');
      assert([200, 304].includes((await page.reload()).status()));
      assert.equal(await page.locator('.balances tbody tr').count(), 3);
      assert.deepEqual(errors, []);
      await context.close();
    }
    const plain = await browser.newPage({viewport:{width:390,height:844},javaScriptEnabled:false});
    await plain.route('**/*', route => route.request().resourceType() === 'image' ? route.abort() : route.continue());
    await plain.goto(url);
    await plain.locator('.mobile-nav summary').click();
    assert(await plain.locator('.mobile-nav').getByRole('link',{name:'Applications', exact:true}).isVisible());
    await plain.locator('.mobile-nav summary').click();
    await plain.locator('.calculation-details summary').click();
    assert(await plain.getByRole('table', {name:'Worked example with fictitious companies'}).isVisible());
    assert(await plain.getByRole('cell', {name:'70 TEST'}).first().isVisible());
    await plain.close();
    console.log(engine + ': four viewport sizes, keyboard, navigation, disclosure, 200% text and offline-image fallback passed.');
  } finally {
    await browser.close();
  }
}
