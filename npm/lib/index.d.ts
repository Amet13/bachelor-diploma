export interface RoundFile {
  $schema: string;
  schema_version: number;
  round: { name: string; created: string; anonymized: boolean; source: Record<string, unknown> };
  items: Array<Record<string, unknown>>;
}
export function load(): Promise<string>;
export function importReview(
  pdf: Uint8Array,
  opts: { synctex?: Uint8Array; sources: Record<string, string>; round: string; anonymize?: boolean },
): RoundFile;
export function patch(round: RoundFile, sources: Record<string, string>, ids?: string[]): string;
export function exportRound(round: RoundFile, format: 'markdown' | 'letter_markdown' | 'letter_latex' | 'sarif'): string;
export function answer(pdf: Uint8Array, round: RoundFile, author?: string): Uint8Array;
export function resolveItem(
  round: RoundFile,
  sources: Record<string, string>,
  id: string,
  opts?: { status?: string; reply?: string; author?: string },
): RoundFile;
