'use strict';
// Requires `cargo xtask npm` and `cargo xtask sample`.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const pb = require('../lib/index.js');

const repo = path.join(__dirname, '..', '..');

test('Node import matches the native import exactly', async () => {
  await pb.load();
  const fx = path.join(repo, 'fixtures', 'thesis-long');
  const sources = {};
  for (const f of ['main.tex', ...fs.readdirSync(path.join(fx, 'chapters')).map((c) => `chapters/${c}`)]) {
    sources[f] = fs.readFileSync(path.join(fx, f), 'utf8');
  }
  const rf = pb.importReview(fs.readFileSync(path.join(repo, 'target', 'sample', 'review.pdf')), {
    synctex: fs.readFileSync(path.join(fx, 'main.synctex.gz')),
    sources,
    round: 'node-test',
  });
  const expected = JSON.parse(fs.readFileSync(path.join(repo, 'target', 'sample', 'expected.json'), 'utf8'));
  assert.strictEqual(rf.items.length, expected.length);
  const byNm = new Map(rf.items.map((i) => [i.origin.nm, i]));
  for (const e of expected) {
    const g = byNm.get(e.nm);
    assert.ok(g, `missing ${e.nm}`);
    assert.strictEqual(g.anchor.confidence, e.confidence, e.nm);
    assert.strictEqual(g.target ? g.target.range.start : null, e.start, e.nm);
    assert.strictEqual(g.target ? g.target.range.end : null, e.end, e.nm);
  }
  assert.match(pb.exportRound(rf, 'markdown'), /^- \[ \] `pb_/);
  const diff = pb.patch(rf, sources);
  assert.match(diff, /sharper wording/);
});
