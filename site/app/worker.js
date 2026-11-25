// proofback engine worker: PDFium (WebAssembly) plus the proofback core.
// Runs entirely in this browser tab; nothing is sent anywhere.
/* global wasm_bindgen, Module */
self.Module = {
  locateFile: (path) => 'engine/' + path,
};
let ready = null;

function start() {
  if (ready) return ready;
  ready = (async () => {
    importScripts('engine/pdfium.js', 'engine/proofback_wasm.js');
    await new Promise((resolve) => {
      if (self.Module.calledRun) resolve();
      else self.Module.onRuntimeInitialized = resolve;
    });
    const rust = await wasm_bindgen({ module_or_path: 'engine/proofback_wasm_bg.wasm' });
    if (!wasm_bindgen.initialize_pdfium_render(self.Module, rust, false)) {
      throw new Error('could not start PDFium');
    }
    return wasm_bindgen.version();
  })();
  return ready;
}

const ops = {
  start: async () => start(),
  readZip: async ({ bytes }) => JSON.parse(wasm_bindgen.read_zip(new Uint8Array(bytes))),
  zipEntry: async ({ bytes, name }) => wasm_bindgen.zip_entry(new Uint8Array(bytes), name),
  import: async ({ pdf, synctex, sources, round, anonymize }) =>
    wasm_bindgen.import_review(new Uint8Array(pdf), synctex ? new Uint8Array(synctex) : undefined, JSON.stringify(sources), round, anonymize, new Date().toISOString().replace(/\.\d+Z$/, 'Z')),
  resolve: async ({ round, sources, id, status, reply, author }) =>
    wasm_bindgen.resolve_item(round, JSON.stringify(sources), id, status || '', reply || '', author, new Date().toISOString().replace(/\.\d+Z$/, 'Z')),
  patch: async ({ round, sources, ids }) => wasm_bindgen.patch(round, JSON.stringify(sources), JSON.stringify(ids || [])),
  export: async ({ round, format }) => wasm_bindgen.export_round(round, format),
  answer: async ({ pdf, round, author }) => wasm_bindgen.answer(new Uint8Array(pdf), round, author),
  render: async ({ pdf, page, width }) => wasm_bindgen.render_page(new Uint8Array(pdf), page, width),
  pageSize: async ({ pdf, page }) => Array.from(wasm_bindgen.page_size(new Uint8Array(pdf), page)),
};

self.onmessage = async ({ data }) => {
  const { id, op, args } = data;
  try {
    if (op !== 'start') await start();
    const result = await ops[op](args || {});
    const transfer = result instanceof Uint8Array ? [result.buffer] : [];
    self.postMessage({ id, ok: true, result }, transfer);
  } catch (e) {
    self.postMessage({ id, ok: false, error: String(e && e.message ? e.message : e) });
  }
};
