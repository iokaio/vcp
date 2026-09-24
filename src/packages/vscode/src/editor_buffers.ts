// SPDX-License-Identifier: Apache-2.0
import { createHash, randomUUID } from 'node:crypto';
import { realpathSync } from 'node:fs';
import path from 'node:path';
import { languages, Range, type TextDocument, type TextEditor } from 'vscode';

export interface BufferBinding {
  readonly host: string; readonly rootId: string; readonly rootPath: string;
  readonly bindingRevision: string; readonly generation: number; readonly trusted: boolean;
}
export interface BufferPosition { readonly line: number; readonly character: number }
export interface BufferRange { readonly start: BufferPosition; readonly end: BufferPosition }
export interface BufferEdit { readonly range: BufferRange; readonly text: string }
export interface BufferDiagnostics {
  readonly collector: string; readonly observed_at: string; readonly observed_document_version: string;
  readonly producer_document_version: null; readonly count: number; readonly truncated: boolean; readonly sha256: string;
}
export interface BufferDocument {
  readonly host: string; readonly open_id: string; readonly uri: string; readonly relative_path: string;
  readonly version: string; readonly content_sha256: string; readonly dirty: boolean;
  readonly content: string | null; readonly disk_sha256: null; readonly language: string;
  readonly eol: 'lf' | 'crlf'; readonly encoding: 'unknown'; readonly selections: readonly BufferRange[]; readonly capture: false;
  readonly diagnostics: BufferDiagnostics | null;
}
export interface BufferCapture { readonly document: BufferDocument; readonly binding: BufferBinding; readonly epoch: number }
export interface BufferApplyResult {
  readonly outcome: 'applied' | 'rejected' | 'unknown'; readonly before: BufferCapture;
  readonly after?: BufferCapture; readonly reason: string;
}
const MAX_BYTES = 65536;
const malformedUnicode = (text: string) => /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/u.test(text);
const hash = (text: string) => createHash('sha256').update(text, 'utf8').digest('hex');
const plainPath = (value: string) => value.startsWith('\\\\?\\') ? value.slice(4) : value;
const comparable = (value: string) => process.platform === 'win32' ? plainPath(value).toLowerCase() : value;
const sameBinding = (a: BufferBinding, b: BufferBinding) => a.host === b.host && a.rootId === b.rootId
  && comparable(a.rootPath) === comparable(b.rootPath) && a.bindingRevision === b.bindingRevision && a.generation === b.generation;
function mapped(document: TextDocument, binding: BufferBinding): string {
  if (document.isClosed || document.uri.scheme !== 'file' || document.uri.authority || document.uri.query || document.uri.fragment || !path.isAbsolute(plainPath(binding.rootPath))) throw new Error('A current local file in the selected root is required.');
  const root = plainPath(realpathSync(plainPath(binding.rootPath)));
  if (comparable(path.resolve(plainPath(binding.rootPath))) !== comparable(root)) throw new Error('Selected root mapping changed.');
  const file = plainPath(realpathSync(document.uri.fsPath));
  if (comparable(path.resolve(document.uri.fsPath)) !== comparable(file)) throw new Error('Aliased document paths require fresh canonical review.');
  const relative = path.relative(root, file);
  if (!relative || relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) throw new Error('Document is outside the selected canonical root.');
  return relative.split(path.sep).join('/');
}
function offset(text: string, position: BufferPosition): number {
  if (!Number.isSafeInteger(position.line) || !Number.isSafeInteger(position.character) || position.line < 0 || position.character < 0) throw new Error('Invalid UTF-16 range.');
  const lines = text.split(/\r\n|\r|\n/);
  const line = lines[position.line];
  if (line === undefined || position.character > line.length) throw new Error('Range exceeds the document.');
  const character = position.character;
  if (character > 0 && character < line.length && /[\uD800-\uDBFF]/.test(line[character - 1]!) && /[\uDC00-\uDFFF]/.test(line[character]!)) throw new Error('Range splits a Unicode character.');
  const breaks = [...text.matchAll(/\r\n|\r|\n/g)];
  return position.line === 0 ? character : breaks[position.line - 1]!.index! + breaks[position.line - 1]![0].length + character;
}
function range(text: string, value: BufferRange): BufferRange {
  if (offset(text, value.start) > offset(text, value.end)) throw new Error('Reversed edit range.');
  return Object.freeze({ start: Object.freeze({ line: value.start.line, character: value.start.character }), end: Object.freeze({ line: value.end.line, character: value.end.character }) });
}
function intended(text: string, edits: readonly BufferEdit[], eol: 'lf' | 'crlf'): { text: string; edits: readonly BufferEdit[] } {
  if (edits.length < 1 || edits.length > 128) throw new Error('Prepared edit count exceeds the supported bound.');
  const normalized = edits.map(edit => {
    if (typeof edit.text !== 'string' || malformedUnicode(edit.text) || Buffer.byteLength(edit.text, 'utf8') > MAX_BYTES) throw new Error('Prepared text exceeds the supported UTF-8 bound.');
    const editRange = range(text, edit.range);
    return { range: editRange, text: edit.text.replace(/\r\n|\r|\n/g, eol === 'crlf' ? '\r\n' : '\n'), start: offset(text, editRange.start), end: offset(text, editRange.end) };
  }).sort((a, b) => a.start - b.start || a.end - b.end);
  for (let i = 1; i < normalized.length; i++) {
    const prior = normalized[i - 1]!, current = normalized[i]!;
    if (current.start < prior.end || current.start === prior.start) throw new Error('Overlapping or ambiguous edit ranges.');
  }
  let output = text;
  for (const edit of [...normalized].reverse()) output = output.slice(0, edit.start) + edit.text + output.slice(edit.end);
  if (Buffer.byteLength(output, 'utf8') > MAX_BYTES) throw new Error('Result exceeds the supported document bound.');
  if (output === text) throw new Error('Prepared edit makes no content change.');
  return { text: output, edits: normalized };
}

function diagnostics(document: TextDocument): BufferDiagnostics | null {
  try {
    const source = languages.getDiagnostics(document.uri);
    const rows: string[] = []; let bytes = 2, truncated = source.length > 64;
    for (const diagnostic of source.slice(0, 64)) {
      // Bound fields before serialization as language servers can return huge messages.
      const code = typeof diagnostic.code === 'object' ? diagnostic.code.value : diagnostic.code;
      const fields = [diagnostic.message, diagnostic.source ?? '', typeof code === 'string' ? code : ''];
      if (fields.some(value => value.length > MAX_BYTES)) { truncated = true; break; }
      const row = JSON.stringify({ range: { start: { line: diagnostic.range.start.line, character: diagnostic.range.start.character },
        end: { line: diagnostic.range.end.line, character: diagnostic.range.end.character } },
        source: diagnostic.source ?? null, severity: diagnostic.severity, code: code ?? null, message: diagnostic.message });
      const size = Buffer.byteLength(row, 'utf8') + (rows.length ? 1 : 0);
      if (bytes + size > MAX_BYTES) { truncated = true; break; }
      rows.push(row); bytes += size;
    }
    return Object.freeze({ collector: 'vscode.languages.getDiagnostics', observed_at: String(Date.now()),
      observed_document_version: String(document.version), producer_document_version: null,
      count: rows.length, truncated, sha256: hash(`[${rows.sort().join(',')}]`) });
  } catch { return null; }
}

/** Ephemeral document identity and version fence; disk authority belongs to the engine. */
export class EditorBuffers {
  #documents = new WeakMap<TextDocument, { id: string; epoch: number }>();
  #disposed = false;
  invalidate(document: TextDocument): void { const value = this.#documents.get(document); if (value) value.epoch++; }
  dispose(): void { this.#disposed = true; this.#documents = new WeakMap(); }
  capture(editor: Pick<TextEditor, 'document' | 'selections'>, binding: BufferBinding, includeContent = true): BufferCapture {
    if (this.#disposed) throw new Error('Document adapter is disposed.');
    const document = editor.document;
    const relative = mapped(document, binding), text = document.getText();
    if (malformedUnicode(text) || Buffer.byteLength(text, 'utf8') > MAX_BYTES || document.languageId.length > 128 || editor.selections.length > 32 || !Number.isSafeInteger(document.version) || document.version < 1) throw new Error('Document observation exceeds supported bounds.');
    let identity = this.#documents.get(document);
    if (!identity) { identity = { id: randomUUID(), epoch: 0 }; this.#documents.set(document, identity); }
    const observation: BufferDocument = Object.freeze({ host: binding.host, open_id: identity.id, uri: document.uri.toString(), relative_path: relative,
      version: String(document.version), content_sha256: hash(text), dirty: document.isDirty, content: includeContent ? text : null,
      disk_sha256: null, language: document.languageId, eol: document.eol === 2 ? 'crlf' : 'lf', encoding: 'unknown',
      selections: Object.freeze(editor.selections.map(selection => range(text, selection))), capture: false, diagnostics: diagnostics(document) });
    return Object.freeze({ document: observation, binding: Object.freeze({ ...binding }), epoch: identity.epoch });
  }
  async apply(editor: TextEditor, expected: BufferCapture, edits: readonly BufferEdit[], liveGuard: () => BufferBinding | undefined): Promise<BufferApplyResult> {
    let before = expected;
    const observe = (): BufferCapture => {
      const binding = liveGuard();
      if (!binding || !binding.trusted || !expected.binding.trusted || !sameBinding(binding, expected.binding)) throw new Error('Trust or workspace binding changed.');
      return this.capture(editor, binding);
    };
    const checked = (): BufferCapture => {
      const current = observe();
      if (current.document.open_id !== expected.document.open_id || current.document.uri !== expected.document.uri
        || current.document.relative_path !== expected.document.relative_path || current.epoch !== expected.epoch
        || current.document.version !== expected.document.version || current.document.content_sha256 !== expected.document.content_sha256
        || current.document.dirty !== expected.document.dirty) throw new Error('Document observation changed; review again.');
      return current;
    };
    let prepared: ReturnType<typeof intended>;
    try { before = checked(); prepared = intended(before.document.content!, edits, before.document.eol); }
    catch (error) {
      let after: BufferCapture | undefined;
      try { after = observe(); } catch { /* No observation across lost authority. */ }
      return { outcome: 'rejected', before, ...(after ? { after } : {}), reason: error instanceof Error ? error.message : 'Document validation failed.' };
    }
    let validationRejected = false;
    try {
      // No await between the final expected-state check and the editor's builder.
      const applied = await editor.edit(builder => {
        try { checked(); } catch (error) { validationRejected = true; throw error; }
        for (const edit of prepared.edits) builder.replace(new Range(edit.range.start.line, edit.range.start.character, edit.range.end.line, edit.range.end.character), edit.text);
      }, { undoStopBefore: true, undoStopAfter: true });
      let after: BufferCapture;
      try { after = observe(); } catch { return { outcome: applied ? 'unknown' : 'rejected', before, reason: 'Current document authority or mapping is unavailable.' }; }
      if (!applied) return { outcome: 'rejected', before, after, reason: 'Editor rejected the version-bound edit; review current text.' };
      if (after.document.open_id !== before.document.open_id || after.document.content_sha256 !== hash(prepared.text)
        || BigInt(after.document.version) !== BigInt(before.document.version) + 1n) return { outcome: 'unknown', before, after, reason: 'Document changed during application; reconcile without replay.' };
      return { outcome: 'applied', before, after, reason: 'Observed the prepared content in the editor buffer.' };
    } catch (error) {
      let after: BufferCapture | undefined;
      try { after = observe(); } catch { /* Lost scope cannot authorize a new observation. */ }
      return { outcome: validationRejected ? 'rejected' : 'unknown', before, ...(after ? { after } : {}), reason: validationRejected ? 'Document changed before the edit could be submitted.' : 'Editor outcome is unknown; reconcile without replay.' };
    }
  }
}
