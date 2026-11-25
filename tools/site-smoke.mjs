// Headless smoke test for the website, run in CI on every pull request.
// Serves ../site under /proofback/ (like GitHub Pages) and fails on:
// console errors (including CSP violations), requests to other origins,
// horizontal overflow, or a broken demo.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = resolve(fileURLToPath(new URL('../site', import.meta.url)));
const types = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.svg': 'image/svg+xml',
  '.woff2': 'font/woff2', '.png': 'image/png', '.txt': 'text/plain' };

const server = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x');
  let rel = url.pathname.startsWith('/proofback/') ? url.pathname.slice('/proofback/'.length) : null;
  if (rel === '') rel = 'index.html';
  const file = rel === null ? null : normalize(join(root, decodeURIComponent(rel)));
  try {
    if (!file || !file.startsWith(root)) throw new Error('outside');
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': types[extname(file)] || 'application/octet-stream' }).end(body);
  } catch {
    res.writeHead(404, { 'content-type': 'text/html' }).end(await readFile(join(root, '404.html')));
  }
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}/proofback/`;

const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined });
const failures = [];
const cases = [
  { name: 'desktop light', path: '', viewport: { width: 1366, height: 900 }, colorScheme: 'light' },
  { name: 'desktop dark', path: '', viewport: { width: 1366, height: 900 }, colorScheme: 'dark' },
  { name: 'phone', path: '', viewport: { width: 390, height: 844 }, colorScheme: 'light' },
  { name: '404', path: 'missing-page', viewport: { width: 390, height: 844 }, colorScheme: 'light' },
];

for (const c of cases) {
  const page = await browser.newPage({ viewport: c.viewport, colorScheme: c.colorScheme });
  const fail = (msg) => failures.push(`[${c.name}] ${msg}`);
  page.on('console', (m) => {
    if (m.type() !== 'error') return;
    // The 404 case answers the document itself with status 404, which Chromium logs.
    if (c.name === '404' && /status of 404/.test(m.text()) && m.location().url === base + c.path) return;
    fail(`console error: ${m.text()}`);
  });
  page.on('pageerror', (e) => fail(`page error: ${e.message}`));
  page.on('request', (r) => { if (!r.url().startsWith(base) && !r.url().startsWith('data:')) fail(`external request: ${r.url()}`); });
  await page.goto(base + c.path, { waitUntil: 'networkidle' });
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - innerWidth);
  if (overflow > 0) fail(`horizontal overflow of ${overflow}px`);
  if (c.path === '') {
    for (const id of [1, 2, 3, 4, 5]) {
      await page.click(`.mk[data-id="${id}"]`);
      const pressed = await page.getAttribute(`.mk[data-id="${id}"]`, 'aria-pressed');
      const shown = await page.textContent('#item');
      if (pressed !== 'true' || !/pb_\w+/.test(shown || '')) fail(`demo mark ${id} did not update the item panel`);
    }
  }
  await page.close();
}

await browser.close();
server.close();
if (failures.length) { console.error(failures.join('\n')); process.exit(1); }
console.log(`site smoke test passed (${cases.length} cases)`);
