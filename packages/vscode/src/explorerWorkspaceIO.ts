// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import type { LiveCheck, OpenSourceResult } from './explorerContracts';
import { caseFold, inside, liveCheck, sourceFile } from './explorerSource';

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
