import { test } from 'node:test';
import * as assert from 'node:assert';
import { codePointColumnToUtf16, escapeMarkdown, groupByFile, label, parseList } from '../src/model';

const json = JSON.stringify({
  items: [
    { id: 'pb_a', round: 'r', kind: 'replace', status: 'open', confidence: 'exact', anchor_state: 'anchored', file: 'b.tex', line: 9, column: 3, untrusted_reviewer_text: { note: '', body: 'x\u001b[31m\nnew line' } },
    { id: 'pb_b', round: 'r', kind: 'comment', status: 'done', confidence: 'line', anchor_state: 'anchored', file: 'a.tex', line: 2, untrusted_reviewer_text: { note: '', body: 'ok' } },
    { id: 'pb_c', round: 'r', kind: 'figure', status: 'open', confidence: 'page', anchor_state: 'anchored', page: 4, untrusted_reviewer_text: { note: '', body: '' } },
  ],
});

test('parses and groups items', () => {
  const items = parseList(json);
  assert.strictEqual(items.length, 3);
  const open = groupByFile(items, false);
  assert.deepStrictEqual(open.map(([f]) => f), ['b.tex', '(not located)']);
  assert.strictEqual(groupByFile(items, true)[0][0], 'a.tex');
  assert.throws(() => parseList('{"nope": 1}'));
});

test('labels drop control characters', () => {
  const items = parseList(json);
  assert.strictEqual(label(items[0]), 'x [31m new line');
  assert.strictEqual(label(items[2]), '(no note)');
});

test('columns convert from code points to UTF-16', () => {
  assert.strictEqual(codePointColumnToUtf16('abc', 3), 2);
  assert.strictEqual(codePointColumnToUtf16('𝑥 = strong', 5), 5);
});

test('markdown is escaped', () => {
  assert.strictEqual(escapeMarkdown('[click](javascript:x)'), '\\[click\\]\\(javascript:x\\)');
});
