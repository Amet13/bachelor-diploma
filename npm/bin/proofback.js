#!/usr/bin/env node
'use strict';
// Minimal Node CLI. For the full CLI (triage, MCP server, sync, apply), install the
// native binary: https://github.com/Amet13/proofback/releases
const fs = require('node:fs');
const path = require('node:path');
const pb = require('../lib/index.js');

const USAGE = `usage:
  proofback import <review.pdf> [--synctex FILE] [--dir PROJECT] [--round NAME] [--anonymize]
  proofback export <round.json> --format markdown|letter_markdown|letter_latex|sarif
  proofback patch <round.json> [--dir PROJECT]

import writes .proofback/rounds/<round>.json, readable by the native CLI.`;

function flag(args, name) {
  const i = args.indexOf(name);
  if (i < 0) return undefined;
  const v = args[i + 1];
  args.splice(i, 2);
  return v;
}

function readSources(dir) {
  const out = {};
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      if (e.name.startsWith('.') || e.name === 'node_modules' || e.isSymbolicLink()) continue;
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (/\.(tex|ltx)$/i.test(e.name) && fs.statSync(p).size < 8 * 1024 * 1024) {
        out[path.relative(dir, p).split(path.sep).join('/')] = fs.readFileSync(p, 'utf8');
      }
    }
  };
  walk(dir);
  return out;
}

async function main(argv) {
  const args = argv.slice(2);
  const cmd = args.shift();
  if (!cmd || cmd === '--help' || cmd === '-h') {
    console.log(USAGE);
    return 0;
  }
  if (cmd === '--version') {
    console.log(await pb.load());
    return 0;
  }
  await pb.load();
  if (cmd === 'import') {
    const synctex = flag(args, '--synctex');
    const dir = path.resolve(flag(args, '--dir') || '.');
    const anonymize = args.includes('--anonymize');
    const roundArg = flag(args, '--round');
    const file = args.find((a) => !a.startsWith('--'));
    if (!file) { console.error(USAGE); return 2; }
    const stem = path.basename(file, path.extname(file)).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
    const round = roundArg || `${new Date().toISOString().slice(0, 10)}-${stem || 'review'}`;
    const rf = pb.importReview(fs.readFileSync(file), {
      synctex: synctex ? fs.readFileSync(synctex) : undefined,
      sources: readSources(dir),
      round,
      anonymize,
    });
    const out = path.join(dir, '.proofback', 'rounds', `${round}.json`);
    fs.mkdirSync(path.dirname(out), { recursive: true });
    fs.writeFileSync(out, JSON.stringify(rf, null, 2) + '\n');
    const by = (c) => rf.items.filter((i) => i.anchor.confidence === c).length;
    console.log(`Imported ${rf.items.length} item(s) into ${path.relative(process.cwd(), out)}: ${by('exact')} exact, ${by('line')} line, ${by('region')} region, ${by('page')} page`);
    return 0;
  }
  if (cmd === 'export') {
    const format = flag(args, '--format') || 'markdown';
    const rf = JSON.parse(fs.readFileSync(args[0], 'utf8'));
    process.stdout.write(pb.exportRound(rf, format));
    return 0;
  }
  if (cmd === 'patch') {
    const dir = path.resolve(flag(args, '--dir') || '.');
    const rf = JSON.parse(fs.readFileSync(args[0], 'utf8'));
    process.stdout.write(pb.patch(rf, readSources(dir)));
    return 0;
  }
  console.error(USAGE);
  return 2;
}

main(process.argv).then((code) => process.exit(code), (e) => {
  console.error(`proofback: ${e.message}`);
  process.exit(3);
});
