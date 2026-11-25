// End-to-end test of the browser app: imports the sample review in headless
// Chromium and requires the result to match the native import exactly.
// Prerequisites: `cargo xtask wasm` and `cargo xtask sample`.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const repo = resolve(fileURLToPath(new URL('..', import.meta.url)));
const root = join(repo, 'site');
const sample = join(repo, 'target/sample');
for (const p of [join(root, 'app/engine/proofback_wasm_bg.wasm'), join(sample, 'review.pdf')]) {
  if (!existsSync(p)) { console.error(`missing ${p}; run cargo xtask wasm and cargo xtask sample`); process.exit(2); }
}
const types = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.wasm': 'application/wasm', '.json': 'application/json' };
const server = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x');
  let rel = url.pathname.startsWith('/proofback/') ? url.pathname.slice(11) : null;
  if (rel === '' || rel?.endsWith('/')) rel = (rel || '') + 'index.html';
  const file = rel === null ? null : normalize(join(root, decodeURIComponent(rel)));
  try {
    if (!file || !file.startsWith(root)) throw new Error('outside');
    res.writeHead(200, { 'content-type': types[extname(file)] || 'application/octet-stream' }).end(await readFile(file));
  } catch { res.writeHead(404).end('not found'); }
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}/proofback/`;

const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined });
const page = await browser.newPage({ acceptDownloads: true, viewport: { width: 1440, height: 900 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
page.on('request', (r) => { if (!r.url().startsWith(base) && !r.url().startsWith('blob:') && !r.url().startsWith('data:')) errors.push(`external request ${r.url()}`); });
await page.goto(base + 'app/', { waitUntil: 'networkidle' });

await page.setInputFiles('#pdf-input', join(sample, 'review.pdf'));
await page.setInputFiles('#synctex-input', join(repo, 'fixtures/thesis-long/main.synctex.gz'));
await page.setInputFiles('#dir-input', join(repo, 'fixtures/thesis-long'));
await page.click('#import-btn');
await page.waitForSelector('#review:not([hidden])', { timeout: 120000 }).catch(async (e) => {
  console.error('setup status:', await page.textContent('#setup-status'));
  throw e;
});
const count = await page.locator('#item-list li').count();

const [dl] = await Promise.all([page.waitForEvent('download'), page.click('[data-export="round"]')]);
const round = JSON.parse(await readFile(await dl.path(), 'utf8'));
const expected = JSON.parse(await readFile(join(sample, 'expected.json'), 'utf8'));
const got = new Map(round.items.map((i) => [i.origin.nm, i]));
const mismatches = [];
for (const e of expected) {
  const g = got.get(e.nm);
  const same = g && g.anchor.confidence === e.confidence && (g.target?.file ?? null) === e.file && (g.target?.range.start ?? null) === e.start && (g.target?.range.end ?? null) === e.end;
  if (!same) mismatches.push({ expected: e, got: g && { confidence: g.anchor.confidence, target: g.target && { file: g.target.file, range: g.target.range } } });
}

// The page renders and marks are drawn.
await page.waitForFunction(() => document.getElementById('page-canvas').width > 0, null, { timeout: 60000 });
await page.waitForTimeout(500);
if (process.env.APP_SCREENSHOT) await page.screenshot({ path: process.env.APP_SCREENSHOT });
const marks = await page.locator('#page-stage .mark').count();
const painted = await page.evaluate(() => {
  const c = document.getElementById('page-canvas');
  const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
  let dark = 0;
  for (let i = 0; i < d.length; i += 4 * 3) if (d[i] < 160) dark++;
  return dark;
});

// A decision through the guard rails: a question needs a reply.
await page.locator('#item-list li', { hasText: 'question' }).first().click();
await page.click('[data-status="done"]');
await page.waitForFunction(() => document.getElementById('decide-status').textContent.length > 0, null, { timeout: 30000 });
const refused = await page.textContent('#decide-status');
await page.fill('#reply', 'Yes, see Section 3.4.');
await page.click('[data-status="done"]');
await page.waitForFunction(() => /Marked/.test(document.getElementById('decide-status').textContent), null, { timeout: 30000 }).catch(() => {});
const accepted = await page.textContent('#decide-status');

const [letterDl] = await Promise.all([page.waitForEvent('download'), page.click('[data-export="letter_latex"]')]);
const letterText = await readFile(await letterDl.path(), 'utf8');
const [ans] = await Promise.all([page.waitForEvent('download'), page.click('[data-export="answer"]')]);
const answered = await readFile(await ans.path());

await browser.close();
server.close();

const failures = [];
if (count !== expected.length) failures.push(`expected ${expected.length} items in the list, got ${count}`);
if (mismatches.length) failures.push(`${mismatches.length} item(s) differ from the native import:\n${JSON.stringify(mismatches.slice(0, 5), null, 2)}`);
if (marks < 1) failures.push('no marks drawn on the page');
if (painted < 10) failures.push('the page canvas looks empty');
if (!/reply/i.test(refused || '')) failures.push(`guard rail did not refuse: ${refused}`);
if (!/done/i.test(accepted || '')) failures.push(`decision not saved: ${accepted}`);
if (!letterText.includes('\\documentclass') || !letterText.includes('Section 3.4')) failures.push('LaTeX letter export is wrong');
if (answered.length < 1000 || answered.subarray(0, 5).toString() !== '%PDF-') failures.push('answered PDF is not a PDF');
if (errors.length) failures.push(`console errors:\n${errors.join('\n')}`);
if (failures.length) { console.error(failures.join('\n')); process.exit(1); }
console.log(`app smoke test passed: ${count} items identical to native, ${marks} marks drawn`);
