# proofback (npm)

The proofback engine compiled to WebAssembly, for Node 20+. It maps reviewer
feedback on a PDF to the exact line of your LaTeX source. Everything runs locally.

```bash
npx proofback import review.pdf --synctex main.synctex.gz
```

This writes `.proofback/rounds/<round>.json`, which the
[native CLI](https://github.com/Amet13/proofback/releases) reads too. The native CLI
adds triage, the MCP server, sync and apply.

```js
const pb = require('proofback');
await pb.load();
const round = pb.importReview(pdfBytes, { synctex, sources: { 'main.tex': text }, round: 'r1' });
```

See <https://amet13.github.io/proofback/>.
