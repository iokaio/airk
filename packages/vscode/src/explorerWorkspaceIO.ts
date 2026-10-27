// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import * as path from 'node:path';
import * as vscode from 'vscode';
import type { LiveCheck, OpenSourceResult } from './explorerContracts';
import { caseFold, inside, liveCheck, sourceFile } from './explorerSource';
import { explorerError } from './explorerErrors';
import { EvidenceFailure } from './explorerEvidence';
import { safePrefix, Strategy, STRATEGY_LIMIT_BYTES, validateStrategy } from './explorerStrategies';

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

/** Open rendered Markdown as an untitled document: opened, never saved. */
export async function openPlanDraft(markdown: string): Promise<{ opened: true }> {
    const document = await vscode.workspace.openTextDocument({ content: markdown, language: 'markdown' });
    await vscode.window.showTextDocument(document, { preview: false });
    return { opened: true };
}

export type PlanSaveResult = { saved: true; path: string; hash: string } | { saved: false; reason: string };

/** The workspace setting, accepted only as a relative path inside the folder. */
export function plannerDirectory(): string {
    const value = vscode.workspace.getConfiguration('airk').get<string>('explorer.planner.directory', '.airk/plans');
    return typeof value === 'string' && value.trim() && safePrefix(value.trim()) ? value.trim().replace(/\/+$/, '') : '.airk/plans';
}

/**
 * Save a plan inside a workspace folder. The save dialog is the consent gate; the target must
 * stay inside the chosen folder, an unsaved open copy is never overwritten, an existing file is
 * replaced only after confirmation, and the written bytes are read back and compared.
 */
export async function savePlanInWorkspace(markdown: string, fileName: string): Promise<PlanSaveResult> {
    const folders = (vscode.workspace.workspaceFolders ?? []).filter(folder => folder.uri.scheme === 'file' || folder.uri.scheme === 'vscode-remote');
    if (!folders.length) { return { saved: false, reason: 'Open a workspace folder to save plans in it.' }; }
    const folder = folders.length === 1 ? folders[0] : (await vscode.window.showQuickPick(folders.map(entry => ({ label: entry.name, description: entry.uri.fsPath, folder: entry })),
        { title: 'Save the AIRK plan in which workspace folder?' }))?.folder;
    if (!folder) { return { saved: false, reason: 'No workspace folder was chosen.' }; }
    const defaultUri = vscode.Uri.joinPath(folder.uri, ...plannerDirectory().split('/'), fileName);
    const target = await vscode.window.showSaveDialog({ title: 'Save AIRK plan', defaultUri, filters: { Markdown: ['md'] } });
    if (!target) { return { saved: false, reason: 'Save was canceled; nothing was written.' }; }
    if (target.scheme !== folder.uri.scheme || target.authority !== folder.uri.authority) { return { saved: false, reason: 'Plans can only be saved inside the chosen workspace folder.' }; }
    const fold = (value: string) => target.scheme === 'file' && process.platform === 'win32' ? value.toLowerCase() : value;
    const relative = path.posix.relative(fold(folder.uri.path), fold(target.path));
    if (!relative || relative.startsWith('..') || path.posix.isAbsolute(relative)) { return { saved: false, reason: 'Plans can only be saved inside the chosen workspace folder.' }; }
    // Show the path as the user typed it; the folded form is only for the containment check.
    const shown = target.path.slice(folder.uri.path.replace(/\/$/, '').length + 1);
    const open = vscode.workspace.textDocuments.find(document => document.uri.toString() === target.toString());
    if (open?.isDirty) { return { saved: false, reason: 'That file has unsaved edits in the editor; save or close it first. Nothing was written.' }; }
    let exists = false;
    try { exists = (await vscode.workspace.fs.stat(target)).type === vscode.FileType.File; } catch { exists = false; }
    if (exists) {
        const choice = await vscode.window.showWarningMessage(`Replace ${shown}?`, { modal: true, detail: 'The existing file will be overwritten with this plan.' }, 'Replace');
        if (choice !== 'Replace') { return { saved: false, reason: 'The existing file was kept; nothing was written.' }; }
    }
    const bytes = new TextEncoder().encode(markdown);
    await vscode.workspace.fs.createDirectory(vscode.Uri.joinPath(target, '..'));
    await vscode.workspace.fs.writeFile(target, bytes);
    const hash = createHash('sha256').update(bytes).digest('hex');
    const readBack = createHash('sha256').update(await vscode.workspace.fs.readFile(target)).digest('hex');
    if (readBack !== hash) { return { saved: false, reason: 'The saved file does not match the plan that was written; check the file before handing it off.' }; }
    await vscode.window.showTextDocument(target, { preview: false });
    return { saved: true, path: shown, hash };
}
