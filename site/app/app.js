// proofback browser app. All processing happens in a Web Worker in this tab.
(() => {
  'use strict';
  const $ = (id) => document.getElementById(id);

  // ---------- worker RPC ----------
  const worker = new Worker('worker.js');
  let seq = 0;
  const pending = new Map();
  worker.onmessage = ({ data }) => {
    const p = pending.get(data.id);
    if (!p) return;
    pending.delete(data.id);
    if (data.ok) p.resolve(data.result); else p.reject(new Error(data.error));
  };
  const call = (op, args) => new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve, reject });
    worker.postMessage({ id, op, args });
  });

  // ---------- state ----------
  const state = {
    pdf: null, pdfName: 'review.pdf', synctex: null, sources: {}, zip: null,
    roundJson: null, round: null, selected: 0, page: 0, pageScale: 1, rendering: null,
  };
  const author = 'me';

  // ---------- setup ----------
  const status = (msg, isError) => {
    $('setup-status').textContent = msg;
    $('setup-status').classList.toggle('error', !!isError);
  };
  const ready = () => {
    $('import-btn').disabled = !(state.pdf && Object.keys(state.sources).length);
  };
  const readBuf = (file) => file.arrayBuffer();
  const readText = (file) => file.text();

  $('pdf-input').addEventListener('change', async (e) => {
    const f = e.target.files[0];
    if (!f) return;
    state.pdf = await readBuf(f);
    state.pdfName = f.name;
    status(`PDF: ${f.name}`);
    ready();
  });
  $('synctex-input').addEventListener('change', async (e) => {
    const f = e.target.files[0];
    if (f) state.synctex = await readBuf(f);
    status(f ? `SyncTeX: ${f.name}` : '');
  });
  $('dir-input').addEventListener('change', async (e) => {
    const files = Array.from(e.target.files);
    const sources = {};
    let synctex = null;
    for (const f of files) {
      const parts = (f.webkitRelativePath || f.name).split('/');
      const rel = parts.length > 1 ? parts.slice(1).join('/') : parts[0];
      if (rel.split('/').some((c) => c.startsWith('.'))) continue;
      if (/\.(tex|ltx)$/i.test(rel) && f.size < 8 * 1024 * 1024) sources[rel] = await readText(f);
      else if (!state.synctex && /\.synctex(\.gz)?$/i.test(rel)) synctex = f;
      else if (!state.pdf && /\.pdf$/i.test(rel) && /review|annot|comment/i.test(rel)) { state.pdf = await readBuf(f); state.pdfName = f.name; }
    }
    if (synctex && !state.synctex) state.synctex = await readBuf(synctex);
    state.sources = sources;
    status(`${Object.keys(sources).length} .tex file(s) loaded${state.synctex ? ', SyncTeX found' : ''}.`);
    ready();
  });
  $('zip-input').addEventListener('change', async (e) => {
    const f = e.target.files[0];
    if (!f) return;
    status('Reading the archive…');
    try {
      const bytes = await readBuf(f);
      const out = await call('readZip', { bytes: bytes.slice(0) });
      state.sources = out.sources;
      if (!state.synctex && out.synctex.length) {
        state.synctex = (await call('zipEntry', { bytes: bytes.slice(0), name: out.synctex[0] })).buffer;
      }
      const skipped = out.skipped.length ? ` Skipped: ${out.skipped.join(', ')}.` : '';
      status(`${Object.keys(out.sources).length} .tex file(s) from ${f.name}${state.synctex ? ', SyncTeX found' : ''}.${skipped}`);
    } catch (err) {
      status(err.message, true);
    }
    ready();
  });

  $('import-btn').addEventListener('click', async () => {
    $('import-btn').disabled = true;
    status('Loading the engine (first time only)…');
    try {
      await call('start');
      status('Matching annotations to your source…');
      const today = new Date().toISOString().slice(0, 10);
      const stem = state.pdfName.replace(/\.pdf$/i, '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 50) || 'review';
      const json = await call('import', {
        pdf: state.pdf.slice(0), synctex: state.synctex ? state.synctex.slice(0) : null,
        sources: state.sources, round: `${today}-${stem}`, anonymize: $('anon').checked,
      });
      setRound(json);
      $('setup').hidden = true;
      $('review').hidden = false;
      select(0);
      $('item-list').focus();
    } catch (err) {
      status(err.message, true);
      $('import-btn').disabled = false;
    }
  });

  // ---------- review ----------
  function setRound(json) {
    state.roundJson = json;
    state.round = JSON.parse(json);
    const items = state.round.items;
    const by = (c) => items.filter((i) => i.anchor.confidence === c).length;
    $('round-name').textContent = state.round.round.name;
    $('round-counts').textContent = `${items.length} items · ${by('exact')} exact · ${by('line')} line · ${by('region')} region · ${by('page')} page`;
    renderList();
  }

  const sortedItems = () => {
    const items = state.round.items.slice();
    items.sort((a, b) => (a.origin.page || 0) - (b.origin.page || 0) || (a.origin.rect ? b.origin.rect[3] - a.origin.rect[3] : 0));
    return items;
  };

  function renderList() {
    const list = $('item-list');
    list.textContent = '';
    sortedItems().forEach((it, i) => {
      const li = document.createElement('li');
      li.dataset.index = String(i);
      const n = document.createElement('span'); n.className = 'n'; n.textContent = String(i + 1);
      const head = document.createElement('span');
      const k = document.createElement('span'); k.className = 'k'; k.textContent = it.kind;
      const c = document.createElement('span'); c.className = `tag ${it.anchor.confidence}`; c.textContent = it.anchor.confidence;
      const s = document.createElement('span'); s.className = 'tag st'; s.textContent = it.status;
      head.append(k, c, s);
      const loc = document.createElement('span'); loc.className = 'loc';
      loc.textContent = it.target ? `${it.target.file}:${it.target.line}` : `p. ${it.origin.page || '?'}`;
      const note = document.createElement('span'); note.className = 'note';
      note.textContent = it.body || (it.origin.marked_text ? `on “${it.origin.marked_text}”` : '(no note)');
      li.append(n, head, loc, note);
      li.addEventListener('click', () => select(i));
      list.append(li);
    });
  }

  async function select(i) {
    const items = sortedItems();
    if (!items.length) return;
    state.selected = Math.max(0, Math.min(i, items.length - 1));
    const it = items[state.selected];
    document.querySelectorAll('#item-list li').forEach((li) => li.classList.toggle('on', Number(li.dataset.index) === state.selected));
    const current = document.querySelector('#item-list li.on');
    if (current) current.scrollIntoView({ block: 'nearest' });
    renderDetail(it);
    if (it.origin.page && it.origin.page !== state.page) await renderPage(it.origin.page);
    renderMarks();
  }

  function renderDetail(it) {
    const d = $('detail');
    d.textContent = '';
    const who = document.createElement('div'); who.className = 'who'; who.textContent = it.author || 'Reviewer';
    const body = document.createElement('div'); body.className = 'body'; body.textContent = it.body || '(no note)';
    const meta = document.createElement('div'); meta.className = 'meta';
    meta.textContent = `${it.id} · ${it.kind} · ${it.status} · ${it.anchor.reason}`;
    d.append(who, body);
    for (const t of it.thread || []) {
      const r = document.createElement('div'); r.className = 'meta'; r.textContent = `↳ ${t.author}: ${t.body}`; d.append(r);
    }
    if (it.suggestion) {
      const s = document.createElement('div'); s.className = 'meta';
      s.textContent = it.suggestion.replace_with ? `suggestion: replace with “${it.suggestion.replace_with}”` : `suggestion: insert “${it.suggestion.insert}”`;
      d.append(s);
    }
    for (const r of it.replies || []) {
      const p = document.createElement('div'); p.className = 'meta'; p.textContent = `you: ${r.body}`; d.append(p);
    }
    d.append(meta);
    $('src-conf').textContent = it.anchor.confidence;
    const lines = $('src-lines');
    lines.textContent = '';
    if (!it.target || !state.sources[it.target.file]) {
      $('src-file').textContent = it.target ? it.target.file : 'not located';
      return;
    }
    const t = it.target;
    $('src-file').textContent = `${t.file}:${t.line}`;
    const text = state.sources[t.file];
    const all = text.split('\n');
    const first = Math.max(1, t.line - 6);
    const last = Math.min(all.length, t.end_line + 6);
    lines.style.counterReset = `ln ${first - 1}`;
    // Byte offsets: the source is UTF-8; JavaScript strings are UTF-16.
    const enc = new TextEncoder();
    let offset = enc.encode(all.slice(0, first - 1).join('\n') + (first > 1 ? '\n' : '')).length;
    for (let ln = first; ln <= last; ln++) {
      const lineText = all[ln - 1];
      const li = document.createElement('li');
      const bytes = enc.encode(lineText);
      const start = offset;
      const end = offset + bytes.length;
      const hs = Math.max(t.range.start, start);
      const he = Math.min(t.range.end, end);
      if (ln >= t.line && ln <= t.end_line) li.classList.add('hit');
      if (hs < he || (t.range.start === t.range.end && t.range.start >= start && t.range.start <= end)) {
        const dec = new TextDecoder();
        const a = dec.decode(bytes.slice(0, hs - start));
        const b = dec.decode(bytes.slice(hs - start, he - start));
        const c = dec.decode(bytes.slice(he - start));
        const mark = document.createElement('mark');
        if (b) mark.textContent = b; else { mark.className = 'ins'; mark.dataset.ins = '‸'; }
        li.append(document.createTextNode(a), mark, document.createTextNode(c));
      } else {
        li.textContent = lineText || ' ';
      }
      lines.append(li);
      offset = end + 1;
    }
    const hit = lines.querySelector('li.hit');
    if (hit) hit.scrollIntoView({ block: 'center' });
  }

  async function renderPage(page) {
    state.page = page;
    $('page-label').textContent = `page ${page}`;
    const width = Math.min(1600, Math.round(($('page-stage').parentElement.clientWidth - 28) * (window.devicePixelRatio || 1)));
    const token = Symbol('render');
    state.rendering = token;
    const [buf, size] = await Promise.all([call('render', { pdf: state.pdf.slice(0), page, width }), call('pageSize', { pdf: state.pdf.slice(0), page })]);
    if (state.rendering !== token) return;
    const view = new DataView(buf.buffer, buf.byteOffset, 8);
    const w = view.getUint32(0, true);
    const h = view.getUint32(4, true);
    const canvas = $('page-canvas');
    canvas.width = w;
    canvas.height = h;
    const cssW = w / (window.devicePixelRatio || 1);
    canvas.style.width = `${cssW}px`;
    canvas.style.height = `${h / (window.devicePixelRatio || 1)}px`;
    canvas.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(buf.buffer, buf.byteOffset + 8, w * h * 4), w, h), 0, 0);
    state.pageScale = cssW / size[0];
    state.pageHeight = size[1];
  }

  function renderMarks() {
    const stage = $('page-stage');
    stage.querySelectorAll('.mark').forEach((m) => m.remove());
    const items = sortedItems();
    items.forEach((it, i) => {
      if (it.origin.type !== 'pdf' || it.origin.page !== state.page) return;
      const [x0, y0, x1, y1] = it.origin.rect;
      const s = state.pageScale;
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'mark' + (i === state.selected ? ' on' : '');
      b.style.left = `${x0 * s}px`;
      b.style.top = `${(state.pageHeight - y1) * s}px`;
      b.style.width = `${Math.max(8, (x1 - x0) * s)}px`;
      b.style.height = `${Math.max(8, (y1 - y0) * s)}px`;
      b.setAttribute('aria-label', `Item ${i + 1}`);
      const n = document.createElement('span'); n.textContent = String(i + 1);
      b.append(n);
      b.addEventListener('click', () => select(i));
      stage.append(b);
    });
  }

  // ---------- decisions ----------
  async function decide(statusValue, reply) {
    const it = sortedItems()[state.selected];
    if (!it) return;
    $('decide-status').textContent = '';
    try {
      const json = await call('resolve', { round: state.roundJson, sources: state.sources, id: it.id, status: statusValue, reply, author });
      const sel = state.selected;
      setRound(json);
      await select(sel);
      $('decide-status').textContent = statusValue ? `Marked ${statusValue}.` : 'Reply saved.';
      if (reply) $('reply').value = '';
    } catch (err) {
      $('decide-status').textContent = err.message;
    }
  }
  document.querySelectorAll('[data-status]').forEach((b) => b.addEventListener('click', () => decide(b.dataset.status, $('reply').value.trim())));
  $('reply-btn').addEventListener('click', () => { const r = $('reply').value.trim(); if (r) decide('', r); });

  document.addEventListener('keydown', (e) => {
    if ($('review').hidden || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'INPUT' || e.metaKey || e.ctrlKey) return;
    const map = { j: () => select(state.selected + 1), ArrowDown: () => select(state.selected + 1), k: () => select(state.selected - 1), ArrowUp: () => select(state.selected - 1),
      d: () => decide('done', $('reply').value.trim()), w: () => decide('wontfix', $('reply').value.trim()), x: () => decide('discuss', $('reply').value.trim()), o: () => decide('open', ''),
      r: () => { e.preventDefault(); $('reply').focus(); } };
    if (map[e.key]) { e.preventDefault(); map[e.key](); }
  });

  // ---------- exports ----------
  function download(name, data, type) {
    const blob = new Blob([data], { type });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = name;
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }
  document.querySelectorAll('[data-export]').forEach((b) => b.addEventListener('click', async () => {
    const kind = b.dataset.export;
    const name = state.round.round.name;
    try {
      if (kind === 'round') download(`${name}.json`, state.roundJson, 'application/json');
      else if (kind === 'patch') download(`${name}.diff`, await call('patch', { round: state.roundJson, sources: state.sources, ids: [] }), 'text/x-diff');
      else if (kind === 'answer') download(`${name}-answered.pdf`, await call('answer', { pdf: state.pdf.slice(0), round: state.roundJson, author }), 'application/pdf');
      else {
        const text = await call('export', { round: state.roundJson, format: kind });
        const ext = kind === 'letter_latex' ? 'tex' : 'md';
        download(`${name}-${kind.replace('_', '-')}.${ext}`, text, 'text/plain');
      }
    } catch (err) {
      $('decide-status').textContent = err.message;
    }
  }));

  window.addEventListener('beforeunload', (e) => {
    if (state.round) { e.preventDefault(); e.returnValue = ''; }
  });

  // Offline support after the first visit.
  if ('serviceWorker' in navigator && location.protocol === 'https:') {
    navigator.serviceWorker.register('sw.js').catch(() => {});
  }
})();
