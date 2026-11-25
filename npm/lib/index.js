'use strict';
// proofback for Node: the same Rust core as the CLI and the browser app, compiled to
// WebAssembly, with PDFium's WebAssembly build for PDF text. Runs locally.

const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

let engine = null;

/** Load the engine once. Resolves to the proofback version. */
async function load() {
  if (engine) return engine.version();
  // PDFium's Emscripten loader expects to run as a classic script whose top-level
  // variables are globals (as in a browser); pdfium-render reads them from there.
  const engineDir = path.join(__dirname, '..', 'engine');
  const code = fs.readFileSync(path.join(engineDir, 'pdfium.js'), 'utf8');
  globalThis.Module = { locateFile: (p) => path.join(engineDir, p) };
  const hadRequire = 'require' in globalThis;
  const hadDirname = '__dirname' in globalThis;
  globalThis.require = require;
  globalThis.__dirname = engineDir;
  try {
    vm.runInThisContext(code, { filename: path.join(engineDir, 'pdfium.js') });
  } finally {
    if (!hadRequire) delete globalThis.require;
    if (!hadDirname) delete globalThis.__dirname;
  }
  const pdfium = globalThis.Module;
  await new Promise((resolve) => {
    if (pdfium.calledRun) resolve();
    else pdfium.onRuntimeInitialized = resolve;
  });
  const pb = require(path.join(__dirname, '..', 'engine', 'proofback_wasm.js'));
  if (!pb.initialize_pdfium_render(pdfium, pb.__wasm, false)) {
    throw new Error('could not start PDFium');
  }
  engine = pb;
  return pb.version();
}

const now = () => new Date().toISOString().replace(/\.\d+Z$/, 'Z');
const need = () => {
  if (!engine) throw new Error('call load() first');
  return engine;
};

/**
 * Import an annotated PDF.
 * @param {Uint8Array} pdf
 * @param {{ synctex?: Uint8Array, sources: Record<string, string>, round: string, anonymize?: boolean }} opts
 * @returns {object} round file
 */
function importReview(pdf, opts) {
  const json = need().import_review(pdf, opts.synctex, JSON.stringify(opts.sources), opts.round, !!opts.anonymize, now());
  return JSON.parse(json);
}

/** Unified diff for edit items (all exact ones when `ids` is empty). */
function patch(round, sources, ids = []) {
  return need().patch(JSON.stringify(round), JSON.stringify(sources), JSON.stringify(ids));
}

/** `markdown`, `letter_markdown`, `letter_latex` or `sarif`. */
function exportRound(round, format) {
  return need().export_round(JSON.stringify(round), format);
}

/** Reviewer's PDF with reply notes for answered items. */
function answer(pdf, round, author = 'me') {
  return need().answer(pdf, JSON.stringify(round), author);
}

/** Set status and/or add a reply, with the CLI's guard rails. */
function resolveItem(round, sources, id, { status = '', reply = '', author = 'me' } = {}) {
  return JSON.parse(need().resolve_item(JSON.stringify(round), JSON.stringify(sources), id, status, reply, author, now()));
}

module.exports = { load, importReview, patch, exportRound, answer, resolveItem };
