import { execFile } from 'node:child_process';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { codePointColumnToUtf16, EDIT_KINDS, escapeMarkdown, groupByFile, ItemRow, label, parseList } from './model';

type Node = { kind: 'file'; file: string; items: ItemRow[] } | { kind: 'item'; item: ItemRow };

function run(root: string, args: string[]): Promise<string> {
  const exe = vscode.workspace.getConfiguration('proofback').get<string>('path') || 'proofback';
  return new Promise((resolve, reject) => {
    // execFile: no shell, arguments are passed as-is.
    execFile(exe, ['--color', 'never', ...args], { cwd: root, maxBuffer: 64 * 1024 * 1024, timeout: 120_000 }, (err, stdout, stderr) => {
      if (err) reject(new Error((stderr || err.message).trim()));
      else resolve(stdout);
    });
  });
}

class Items implements vscode.TreeDataProvider<Node> {
  private readonly changed = new vscode.EventEmitter<Node | undefined>();
  readonly onDidChangeTreeData = this.changed.event;
  items: ItemRow[] = [];

  constructor(private readonly root: string) {}

  async refresh(): Promise<void> {
    try {
      this.items = parseList(await run(this.root, ['list', '--json']));
    } catch (e) {
      this.items = [];
      vscode.window.showWarningMessage(`proofback: ${(e as Error).message}`);
    }
    this.changed.fire(undefined);
  }

  getTreeItem(n: Node): vscode.TreeItem {
    if (n.kind === 'file') {
      const t = new vscode.TreeItem(n.file, vscode.TreeItemCollapsibleState.Expanded);
      t.description = `${n.items.length}`;
      t.contextValue = 'file';
      return t;
    }
    const it = n.item;
    const t = new vscode.TreeItem(label(it, 60), vscode.TreeItemCollapsibleState.None);
    t.description = `${it.kind} · ${it.confidence} · ${it.status}${it.line ? ` · L${it.line}` : it.page ? ` · p.${it.page}` : ''}`;
    t.tooltip = new vscode.MarkdownString(hoverText(it));
    t.contextValue = EDIT_KINDS.has(it.kind) && it.confidence === 'exact' ? 'item-edit' : 'item';
    t.iconPath = new vscode.ThemeIcon(it.status === 'done' ? 'check' : it.kind === 'question' ? 'question' : EDIT_KINDS.has(it.kind) ? 'edit' : 'comment');
    if (it.file && it.line) {
      t.command = { command: 'proofback.reveal', title: 'Show', arguments: [it] };
    }
    return t;
  }

  getChildren(n?: Node): Node[] {
    const show = vscode.workspace.getConfiguration('proofback').get<boolean>('showResolved') ?? false;
    if (!n) return groupByFile(this.items, show).map(([file, items]) => ({ kind: 'file', file, items }));
    return n.kind === 'file' ? n.items.map((item) => ({ kind: 'item', item })) : [];
  }
}

function hoverText(it: ItemRow): string {
  const r = it.untrusted_reviewer_text;
  const who = escapeMarkdown(r.author || 'Reviewer');
  const body = escapeMarkdown(r.body || '(no note)');
  const thread = (r.thread || []).map((t) => `\n\n↳ ${escapeMarkdown(t)}`).join('');
  return `**${who}** (${escapeMarkdown(it.kind)}, ${escapeMarkdown(it.confidence)}): ${body}${thread}\n\n\`${it.id}\``;
}

export function activate(ctx: vscode.ExtensionContext): void {
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!folder) return;
  const root = folder.uri.fsPath;
  const provider = new Items(root);
  ctx.subscriptions.push(vscode.window.registerTreeDataProvider('proofback.items', provider));

  const deco = vscode.window.createTextEditorDecorationType({
    backgroundColor: new vscode.ThemeColor('editor.wordHighlightBackground'),
    overviewRulerColor: new vscode.ThemeColor('editorWarning.foreground'),
    overviewRulerLane: vscode.OverviewRulerLane.Right,
  });
  ctx.subscriptions.push(deco);

  const decorate = (editor?: vscode.TextEditor) => {
    if (!editor) return;
    const rel = path.relative(root, editor.document.uri.fsPath).split(path.sep).join('/');
    const ranges: vscode.DecorationOptions[] = provider.items
      .filter((it) => it.file === rel && it.line && it.status !== 'done' && it.status !== 'wontfix')
      .map((it) => ({ range: toRange(editor.document, it), hoverMessage: new vscode.MarkdownString(hoverText(it)) }));
    editor.setDecorations(deco, ranges);
  };
  const refresh = async () => {
    await provider.refresh();
    vscode.window.visibleTextEditors.forEach(decorate);
  };

  const pick = async (n?: Node): Promise<ItemRow | undefined> => {
    if (n && n.kind === 'item') return n.item;
    const choice = await vscode.window.showQuickPick(
      provider.items.map((it) => ({ label: label(it, 80), description: `${it.id} ${it.file ?? ''}:${it.line ?? ''}`, it })),
      { placeHolder: 'Choose an item' },
    );
    return choice?.it;
  };
  const resolve = (status: string) => async (n?: Node) => {
    const it = await pick(n);
    if (!it) return;
    try {
      await run(root, ['resolve', it.id, '--status', status]);
    } catch (e) {
      vscode.window.showErrorMessage(`proofback: ${(e as Error).message}`);
    }
    await refresh();
  };

  ctx.subscriptions.push(
    vscode.commands.registerCommand('proofback.refresh', refresh),
    vscode.commands.registerCommand('proofback.done', resolve('done')),
    vscode.commands.registerCommand('proofback.wontfix', resolve('wontfix')),
    vscode.commands.registerCommand('proofback.discuss', resolve('discuss')),
    vscode.commands.registerCommand('proofback.reply', async (n?: Node) => {
      const it = await pick(n);
      if (!it) return;
      const reply = await vscode.window.showInputBox({ prompt: `Reply to: ${label(it, 80)}` });
      if (!reply) return;
      try {
        await run(root, ['resolve', it.id, '--reply', reply]);
      } catch (e) {
        vscode.window.showErrorMessage(`proofback: ${(e as Error).message}`);
      }
      await refresh();
    }),
    vscode.commands.registerCommand('proofback.apply', async (n?: Node) => {
      const it = await pick(n);
      if (!it) return;
      try {
        const diff = await run(root, ['patch', it.id]);
        const ok = await vscode.window.showInformationMessage(`Apply this change?\n\n${diff.slice(0, 1500)}`, { modal: true }, 'Apply');
        if (ok === 'Apply') await run(root, ['apply', it.id, '--resolve']);
      } catch (e) {
        vscode.window.showErrorMessage(`proofback: ${(e as Error).message}`);
      }
      await refresh();
    }),
    vscode.commands.registerCommand('proofback.sync', async () => {
      try {
        vscode.window.showInformationMessage(`proofback: ${(await run(root, ['sync'])).trim()}`);
      } catch (e) {
        vscode.window.showErrorMessage(`proofback: ${(e as Error).message}`);
      }
      await refresh();
    }),
    vscode.commands.registerCommand('proofback.reveal', async (it: ItemRow) => {
      const doc = await vscode.workspace.openTextDocument(path.join(root, it.file!));
      const editor = await vscode.window.showTextDocument(doc);
      const r = toRange(doc, it);
      editor.selection = new vscode.Selection(r.start, r.end);
      editor.revealRange(r, vscode.TextEditorRevealType.InCenter);
      decorate(editor);
    }),
    vscode.window.onDidChangeActiveTextEditor(decorate),
  );

  const watcher = vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(folder, '.proofback/rounds/*.json'));
  watcher.onDidChange(refresh);
  watcher.onDidCreate(refresh);
  watcher.onDidDelete(refresh);
  ctx.subscriptions.push(watcher);
  void refresh();
}

function toRange(doc: vscode.TextDocument, it: ItemRow): vscode.Range {
  const line = Math.max(0, (it.line ?? 1) - 1);
  const endLine = Math.max(line, (it.end_line ?? it.line ?? 1) - 1);
  const text = doc.lineAt(Math.min(line, doc.lineCount - 1)).text;
  const start = codePointColumnToUtf16(text, it.column ?? 1);
  const endText = doc.lineAt(Math.min(endLine, doc.lineCount - 1)).text;
  return new vscode.Range(line, start, endLine, endLine === line && start === endText.length ? start : endText.length);
}

export function deactivate(): void {}
