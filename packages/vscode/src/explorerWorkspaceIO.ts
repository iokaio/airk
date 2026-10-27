// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import type { LiveCheck, OpenSourceResult } from './explorerContracts';
import { caseFold, inside, liveCheck, sourceFile } from './explorerSource';
import { explorerError } from './explorerErrors';
import { EvidenceFailure } from './explorerEvidence';
import { Strategy, STRATEGY_LIMIT_BYTES, validateStrategy } from './explorerStrategies';

const MESSAGES: Record<LiveCheck, string> = {
    matches_index: 'The saved file matches the indexed bytes.',
    changed_since_index: 'The saved file changed since it was indexed; the cited lines may have moved.',
    not_saved: 'The file has unsaved edits; the index describes the saved version.',
    missing: 'The file is no longer present at its indexed location.',
    outside_workspace: 'The file is outside this window’s workspace folders.',
    unchecked: 'The file was too large to compare with the index.',
};

/**
 * Open a cited source location. Text sources reveal the indexed line; extracted
 * documents (PDF, Office) open the original file, because their line numbers refer
 * to extracted text. A changed file is still opened, with an explicit warning.
 */
export async function openSource(location: { root: string; relative: string; contentHash: string; lineBasis?: string; startLine: number }): Promise<OpenSourceResult> {
    const file = await sourceFile(location.root, location.relative);
    const uri = vscode.Uri.file(file);
    const dirty = vscode.workspace.textDocuments.some(document => document.uri.scheme === 'file' && caseFold(document.uri.fsPath) === caseFold(file) && document.isDirty);
    const live = await liveCheck(file, location.contentHash, dirty);
    if (live === 'missing') { return { live, opened: false, message: MESSAGES[live] }; }
    const inWorkspace = (vscode.workspace.workspaceFolders ?? []).some(folder => folder.uri.scheme === 'file' && inside(folder.uri.fsPath, file));
    const note = inWorkspace ? '' : ` ${MESSAGES.outside_workspace}`;
    if (location.lineBasis && location.lineBasis !== 'source') {
        await vscode.commands.executeCommand('vscode.open', uri);
        return { live, opened: true, message: `${MESSAGES[live]} Line numbers refer to extracted text; cite the original document location.${note}` };
    }
    const line = Math.max(0, location.startLine - 1);
    await vscode.window.showTextDocument(uri, { preview: true, selection: new vscode.Range(line, 0, line, 0) });
    return { live, opened: true, message: MESSAGES[live] + note };
}

/** Saved-bytes check for an indexed file at AIRK's own source location. */
export async function liveAt(root: string, relative: string, contentHash: string): Promise<LiveCheck> {
    const file = await sourceFile(root, relative);
    const dirty = vscode.workspace.textDocuments.some(document => document.uri.scheme === 'file' && caseFold(document.uri.fsPath) === caseFold(file) && document.isDirty);
    return liveCheck(file, contentHash, dirty);
}

const STRATEGY_FILTERS = { 'AIRK strategy': ['json'] };

/** Export through the save dialog, which is the user's consent; nothing is written without it. */
export async function exportStrategy(strategy: Strategy): Promise<{ saved: boolean; name?: string }> {
    const folder = vscode.workspace.workspaceFolders?.[0]?.uri;
    const name = `${strategy.id.split('/').pop()}.strategy.airk.json`;
    const target = await vscode.window.showSaveDialog({ title: 'Export AIRK strategy', filters: STRATEGY_FILTERS, ...(folder ? { defaultUri: vscode.Uri.joinPath(folder, name) } : {}) });
    if (!target) { return { saved: false }; }
    await vscode.workspace.fs.writeFile(target, new TextEncoder().encode(JSON.stringify(strategy, null, 2) + '\n'));
    return { saved: true, name: target.path.split('/').pop() };
}

/** Import one strategy file; it is validated exactly like a template before Studio sees it. */
export async function importStrategy(): Promise<Strategy | undefined> {
    const [source] = await vscode.window.showOpenDialog({ title: 'Import AIRK strategy', canSelectMany: false, filters: STRATEGY_FILTERS }) ?? [];
    if (!source) { return undefined; }
    const stat = await vscode.workspace.fs.stat(source);
    if (stat.size > STRATEGY_LIMIT_BYTES * 2) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', 'The strategy file is larger than 64 KiB.')); }
    let value: unknown;
    try { value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(await vscode.workspace.fs.readFile(source))); }
    catch { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', 'The strategy file is not valid JSON.')); }
    const checked = validateStrategy(value);
    if (!checked.ok) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', checked.errors.slice(0, 5).join('; '))); }
    return checked.strategy;
}
