// Pure helpers (no VS Code imports) so they can be unit-tested with node:test.

/** One row of `proofback list --json`. */
export interface ItemRow {
  id: string;
  round: string;
  kind: string;
  status: string;
  confidence: string;
  anchor_state: string;
  file?: string;
  line?: number;
  column?: number;
  end_line?: number;
  page?: number;
  untrusted_reviewer_text: { note: string; author?: string; body: string; marked_text?: string; thread?: string[] };
}

export const EDIT_KINDS = new Set(['replace', 'delete', 'insert']);
const RESOLVED = new Set(['done', 'wontfix', 'duplicate']);

/** Parse CLI output defensively. */
export function parseList(json: string): ItemRow[] {
  const v = JSON.parse(json);
  if (!v || !Array.isArray(v.items)) throw new Error('unexpected output from proofback list');
  return v.items.filter((i: unknown): i is ItemRow => typeof i === 'object' && i !== null && typeof (i as ItemRow).id === 'string');
}

/** Items grouped by file (unlocated last), sorted by line. */
export function groupByFile(items: ItemRow[], showResolved: boolean): Array<[string, ItemRow[]]> {
  const groups = new Map<string, ItemRow[]>();
  for (const it of items) {
    if (!showResolved && RESOLVED.has(it.status)) continue;
    const key = it.file ?? '(not located)';
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(it);
  }
  for (const list of groups.values()) list.sort((a, b) => (a.line ?? 0) - (b.line ?? 0) || (a.column ?? 0) - (b.column ?? 0));
  return [...groups.entries()].sort(([a], [b]) => (a === '(not located)' ? 1 : b === '(not located)' ? -1 : a.localeCompare(b)));
}

/** One-line label without control characters. */
export function label(it: ItemRow, width = 70): string {
  const body = (it.untrusted_reviewer_text.body || (it.untrusted_reviewer_text.marked_text ? `on "${it.untrusted_reviewer_text.marked_text}"` : '(no note)'))
    .replace(/[\u0000-\u001f\u007f]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  return body.length > width ? body.slice(0, width - 1) + '…' : body;
}

/**
 * proofback columns count Unicode code points; VS Code positions count UTF-16
 * code units. Convert a 1-based code-point column on `lineText` to a 0-based
 * UTF-16 character offset.
 */
export function codePointColumnToUtf16(lineText: string, column1: number): number {
  let units = 0;
  let cps = 0;
  for (const ch of lineText) {
    if (cps >= column1 - 1) break;
    units += ch.length;
    cps += 1;
  }
  return units;
}

/** Escape text for a Markdown hover (VS Code renders it; never trust it). */
export function escapeMarkdown(s: string): string {
  return s.replace(/[\\`*_{}\[\]()#+\-.!|<>~]/g, (c) => '\\' + c);
}
