// proofback site: interactive PDF-to-source demo. No dependencies.
(() => {
  const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

  // chapters/method.tex, starting at line 38
  const SRC = [
    '\\section{Estimating the threshold}\\label{sec:thr}',
    '',
    '% TODO: tighten this paragraph',
    'The estimator is strong under heavy-tailed noise,',
    'which makes it suitable for sensor data collected',
    'in the field. We therefore fix the threshold at',
    '$\\tau = 0.35$.',
    'Earlier experiments with a fixed window were',
    'inconclusive and are omitted here.',
    'Results for the full dataset are given in',
    'Table~\\ref{tab:full}.',
  ];
  const FIRST = 38;

  // ranges: [lineNo, startCol, endCol] (0-based cols, end exclusive)
  const ITEMS = {
    1: { id: 'pb_7k2m9q', kind: 'replace', conf: 'exact', ranges: [[41, 17, 23]], loc: 'method.tex:41:18–23',
         body: 'Replace with “robust”', diff: [['dm', '- The estimator is strong under'], ['dp', '+ The estimator is robust under']] },
    2: { id: 'pb_q81fza', kind: 'comment', conf: 'exact', ranges: [[42, 16, 40]], loc: 'method.tex:42:17–40',
         body: 'Cite the field study here', diff: [['dn', '  needs a \\cite{...}: your call, or your agent’s']] },
    3: { id: 'pb_3xq1fd', kind: 'question', conf: 'exact', ranges: [[44, 0, 13]], loc: 'method.tex:44:1–13',
         body: 'Why this threshold?', diff: [['dn', '  reply: “Chosen by 5-fold CV. See 3.4.”'], ['dn', '  status: discuss']] },
    4: { id: 'pb_d0r4kl', kind: 'delete', conf: 'exact', ranges: [[45, 0, 44], [46, 0, 34]], loc: 'method.tex:45:1–46:34',
         body: 'Strike-out, no note', diff: [['dm', '- Earlier experiments with a fixed window were'], ['dm', '- inconclusive and are omitted here.']] },
    5: { id: 'pb_v6c2nw', kind: 'insert', conf: 'exact', ranges: [[48, 20, 20, ' and Figure~\\ref{fig:full}']], loc: 'method.tex:48:21',
         body: 'Insert “and Fig. 3.2”', diff: [['dm', '- Table~\\ref{tab:full}.'], ['dp', '+ Table~\\ref{tab:full} and Figure~\\ref{fig:full}.']],
         note: '“Table 3.1” in the PDF ⇄ Table~\\ref{tab:full} in source' },
  };

  const code = document.getElementById('code');
  const item = document.getElementById('item');
  const badge = document.getElementById('badge');
  const desk = document.getElementById('desk');
  const path = document.getElementById('thread-path');
  const dot = document.getElementById('thread-dot');
  if (!code || !item) return;

  function hilite(s) {
    // tiny LaTeX highlighter on already-escaped text
    if (/^\s*%/.test(s)) return `<span class="c">${s}</span>`;
    return s
      .replace(/(\\[a-zA-Z]+)/g, '<span class="k">$1</span>')
      .replace(/(\$[^$]*\$)/g, '<span class="m">$1</span>');
  }

  function renderLine(text, ranges) {
    if (!ranges.length) return hilite(esc(text)) || ' ';
    let out = '', pos = 0;
    for (const [s, e, ins] of ranges.sort((a, b) => a[0] - b[0])) {
      out += hilite(esc(text.slice(pos, s)));
      if (ins) out += `<mark class="ins" data-ins="${esc(ins)}"></mark>`;
      else out += `<mark>${esc(text.slice(s, e))}</mark>`;
      pos = e;
    }
    return out + hilite(esc(text.slice(pos)));
  }

  function render(activeId) {
    const it = ITEMS[activeId];
    const byLine = {};
    if (it) for (const [ln, s, e, ins] of it.ranges) (byLine[ln] ||= []).push([s, e, ins]);
    code.innerHTML = SRC.map((t, i) => {
      const ln = FIRST + i;
      const r = byLine[ln] || [];
      return `<li data-ln="${ln}"${r.length ? ' class="on"' : ''}>${renderLine(t, r)}</li>`;
    }).join('');
    if (!it) return;
    badge.textContent = it.conf;
    badge.className = 'bar-dim' + (it.conf === 'exact' ? '' : ' line');
    item.innerHTML =
      `<div class="row1"><span class="id">${it.id}</span><span class="kind">${it.kind}</span><span class="loc">${esc(it.loc)}</span></div>` +
      `<div class="body">${esc(it.body)}</div>` +
      `<pre class="diff">${it.diff.map(([c, l]) => `<span class="${c}">${esc(l)}</span>`).join('\n')}${it.note ? `\n<span class="dn">  ${esc(it.note)}</span>` : ''}</pre>`;
  }

  const marks = [...document.querySelectorAll('.mk')];
  const notes = [...document.querySelectorAll('.popups [data-for]')];
  let current = 1;

  function drawThread() {
    if (!desk || !path || getComputedStyle(path.parentNode).display === 'none') return;
    const m = document.querySelector(`.mk[data-id="${current}"]`);
    const pop = document.querySelector(`.pop[data-for="${current}"]`);
    const from = pop && pop.offsetParent ? pop : m;
    const line = code.querySelector('li.on');
    if (!m || !line) return;
    const d = desk.getBoundingClientRect();
    const a = from === m ? (m.getClientRects()[0] || m.getBoundingClientRect()) : from.getBoundingClientRect();
    const b = line.getBoundingClientRect();
    const x1 = a.right - d.left + (from === m ? 6 : 0), y1 = a.top + a.height / 2 - d.top;
    const x2 = b.left - d.left + 4, y2 = b.top + b.height / 2 - d.top;
    const cx = (x1 + x2) / 2;
    path.setAttribute('d', `M${x1},${y1} C${cx},${y1} ${cx},${y2} ${x2},${y2}`);
    dot.setAttribute('cx', x2); dot.setAttribute('cy', y2);
  }

  function select(id, fromUser) {
    current = id;
    marks.forEach((m) => m.classList.toggle('on', +m.dataset.id === id));
    notes.forEach((n) => n.classList.toggle('on', +n.dataset.for === id));
    render(id);
    requestAnimationFrame(drawThread);
  }

  marks.forEach((m) => m.addEventListener('click', () => select(+m.dataset.id, true)));
  addEventListener('resize', () => requestAnimationFrame(drawThread));
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(drawThread);

  select(1, false);

  // theme toggle
  const btn = document.querySelector('.theme');
  btn && btn.addEventListener('click', () => {
    const root = document.documentElement;
    const dark = root.dataset.theme ? root.dataset.theme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
    root.dataset.theme = dark ? 'light' : 'dark';
    try { localStorage.setItem('pb-theme', root.dataset.theme); } catch (e) {}
  });
})();
