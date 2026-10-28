// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as vscode from 'vscode';
import type { LiveCheck, OpenSourceResult, WorkspaceFileKind, WorkspaceFilesView } from './explorerContracts';
import { AGENT_INSTRUCTION_FILES } from './explorerContracts';
import { caseFold, displayPath, inside, liveCheck, sourceFile } from './explorerSource';
import { explorerError } from './explorerErrors';
import { EvidenceFailure } from './explorerEvidence';
import { PLAN_LIMIT_BYTES, safePrefix, SearchRecipe, Strategy, validateRecipe, validateStrategy } from './explorerStrategies';
import { ImportedPlan, parsePlanFile } from './explorerPlanFiles';
import { StudioDocument, STUDIO_LIMIT_BYTES, STUDIO_SCHEMA, validateStudio } from './explorerStrategies';

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
        await vscode.commands.executeCommand('vscode.open', uri, { viewColumn: vscode.ViewColumn.Beside });
        return { live, opened: true, message: `${MESSAGES[live]} Line numbers refer to extracted text; cite the original document location.${note}` };
    }
    const line = Math.max(0, location.startLine - 1);
    await vscode.window.showTextDocument(uri, { viewColumn: vscode.ViewColumn.Beside, preview: true, selection: new vscode.Range(line, 0, line, 0) });
    return { live, opened: true, message: MESSAGES[live] + note };
}

/** Saved-bytes check for an indexed file at AIRK's own source location. */
export async function liveAt(root: string, relative: string, contentHash: string): Promise<LiveCheck> {
    const file = await sourceFile(root, relative);
    const dirty = vscode.workspace.textDocuments.some(document => document.uri.scheme === 'file' && caseFold(document.uri.fsPath) === caseFold(file) && document.isDirty);
    return liveCheck(file, contentHash, dirty);
}

const STRATEGY_FILTERS = { 'AIRK strategy': ['json'] };

/** Open instructions for review; missing files become named, unsaved drafts. Never write or append. */
export async function openAgentInstructions(root: string, agent: keyof typeof AGENT_INSTRUCTION_FILES, authorized: () => boolean): Promise<{ message: string }> {
    if (!Object.hasOwn(AGENT_INSTRUCTION_FILES, agent)) { throw new Error('Choose a supported instruction file.'); }
    const folder = vscode.workspace.workspaceFolders?.find(entry => entry.uri.scheme === 'file'
        && caseFold(path.resolve(entry.uri.fsPath)) === caseFold(path.resolve(displayPath(root))));
    const check = () => {
        if (!authorized() || !vscode.workspace.isTrusted) { throw new Error('Workspace or selected project changed; open the instructions again.'); }
        if (!folder || !vscode.workspace.workspaceFolders?.some(entry => entry.uri.toString() === folder.uri.toString())) { throw new Error('Open this project as a workspace folder before editing its instructions.'); }
    };
    check();
    const relative = AGENT_INSTRUCTION_FILES[agent].file;
    const base = fs.realpathSync.native(folder!.uri.fsPath);
    let current = folder!.uri.fsPath, missing = false;
    const parts = relative.split('/');
    for (const [index, part] of parts.entries()) {
        current = path.join(current, part);
        try {
            const stat = fs.lstatSync(current);
            if (stat.isSymbolicLink() || !inside(base, fs.realpathSync.native(current))) { throw new Error('Linked or redirected instruction paths must be opened manually.'); }
            if (index === parts.length - 1 ? !stat.isFile() : !stat.isDirectory()) { throw new Error('The instruction path is not a regular file or folder.'); }
        } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') { missing = true; } else { throw error; } }
    }
    const target = vscode.Uri.file(current);
    const document = await vscode.workspace.openTextDocument(missing ? target.with({ scheme: 'untitled' }) : target);
    check();
    await vscode.window.showTextDocument(document, { viewColumn: vscode.ViewColumn.Beside, preview: false });
    return { message: missing ? `${relative} opened as a new unsaved file. Paste the instructions, review and save.` : `${relative} opened. Add the suggestion alongside your existing instructions, then save.` };
}

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
export async function importStrategy(): Promise<Strategy | StudioDocument | undefined> {
    const folder = vscode.workspace.workspaceFolders?.[0]?.uri;
    const [source] = await vscode.window.showOpenDialog({ title: 'Import AIRK strategy', canSelectMany: false, filters: STRATEGY_FILTERS,
        ...(folder ? { defaultUri: vscode.Uri.joinPath(folder, '.airk', 'strategies') } : {}) }) ?? [];
    if (!source) { return undefined; }
    let value: unknown;
    try { value = JSON.parse(await readImport(source, STUDIO_LIMIT_BYTES * 2)); }
    catch { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', 'The strategy file is not valid JSON.')); }
    return parseStrategyDocument(value);
}

function parseStrategyDocument(value: unknown): Strategy | StudioDocument {
    if ((value as { schema?: string } | null)?.schema === STUDIO_SCHEMA) {
        const document = validateStudio(value);
        if (!document) { throw new Error('Invalid AIRK Studio document.'); }
        return document;
    }
    const checked = validateStrategy(value);
    if (!checked.ok) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', checked.errors.slice(0, 5).join('; '))); }
    return checked.strategy;
}

/** Read only the explicitly selected file, with a real byte ceiling for local files. */
async function readImport(source: vscode.Uri, limit: number): Promise<string> {
    let bytes: Uint8Array;
    if (source.scheme === 'file') {
        const handle = fs.openSync(source.fsPath, fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW ?? 0) | (fs.constants.O_NONBLOCK ?? 0));
        try {
            if (!fs.fstatSync(handle).isFile()) { throw new Error('Choose a regular file.'); }
            const buffer = Buffer.alloc(limit + 1); let used = 0, read = 0;
            do { read = fs.readSync(handle, buffer, used, buffer.length - used, null); used += read; } while (read && used < buffer.length);
            if (used > limit) { throw new Error(`The selected file exceeds ${limit} bytes.`); }
            bytes = buffer.subarray(0, used);
        } finally { fs.closeSync(handle); }
    } else {
        if ((await vscode.workspace.fs.stat(source)).size > limit) { throw new Error(`The selected file exceeds ${limit} bytes.`); }
        bytes = await vscode.workspace.fs.readFile(source);
        if (bytes.byteLength > limit) { throw new Error(`The selected file exceeds ${limit} bytes.`); }
    }
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
}

export async function importPlan(): Promise<ImportedPlan | undefined> {
    const folder = vscode.workspace.workspaceFolders?.[0]?.uri;
    const [source] = await vscode.window.showOpenDialog({ title: 'Load AIRK plan', canSelectMany: false, filters: { 'AIRK plans': ['md', 'json'] },
        ...(folder ? { defaultUri: vscode.Uri.joinPath(folder, ...plannerDirectory().split('/')) } : {}) }) ?? [];
    try { return source ? parsePlanFile(await readImport(source, PLAN_LIMIT_BYTES * 2)) : undefined; }
    catch (error) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', `The plan could not be loaded. ${error instanceof Error ? error.message : String(error)}`)); }
}

const fileBase = (kind: WorkspaceFileKind) => kind === 'plans' ? plannerDirectory() : `.airk/${kind}`;
async function checkRemoteLibrary(root: vscode.Uri, kind: WorkspaceFileKind, relative: string, directory: boolean): Promise<void> {
    const parts = [...relativeFile(fileBase(kind)), ...relativeFile(relative, true)];
    for (let at = 0; at < parts.length; at++) {
        const stat = await vscode.workspace.fs.stat(vscode.Uri.joinPath(root, ...parts.slice(0, at + 1)));
        const expected = at < parts.length - 1 || directory ? vscode.FileType.Directory : vscode.FileType.File;
        if (stat.type !== expected) { throw new Error('Linked or unsupported library paths are not supported by this provider.'); }
    }
}
function relativeFile(value: string, empty = false): string[] {
    if ((!value && !empty) || value.length > 1024 || value.includes('\\') || value.includes(':') || value.startsWith('/')
        || value.split('/').some(part => part === '.' || part === '..' || (!part && value !== '')) || /[\u0000-\u001f\u007f]/.test(value)) { throw new Error('Choose a relative file or folder beneath the library.'); }
    return value ? value.split('/') : [];
}
function libraryTarget(kind: WorkspaceFileKind, workspace: string | undefined, relative: string, preferredRoot?: string): { uri: vscode.Uri; root: vscode.Uri; workspace: string; roots: { id: string; name: string }[] } {
    if (!vscode.workspace.isTrusted) { throw new Error('Trust this workspace before browsing saved files.'); }
    const folders = (vscode.workspace.workspaceFolders ?? []).filter(folder => ['file', 'vscode-remote'].includes(folder.uri.scheme));
    const folder = workspace ? folders.find(entry => entry.uri.toString() === workspace)
        : folders.find(entry => preferredRoot && caseFold(entry.uri.fsPath) === caseFold(displayPath(preferredRoot))) ?? folders[0];
    if (!folder) { throw new Error('Choose an open workspace folder.'); }
    const uri = vscode.Uri.joinPath(folder.uri, ...relativeFile(fileBase(kind)), ...relativeFile(relative, true));
    if (uri.scheme === 'file') {
        const base = fs.realpathSync.native(folder.uri.fsPath);
        let current = folder.uri.fsPath;
        for (const part of path.relative(current, uri.fsPath).split(path.sep).filter(Boolean)) {
            current = path.join(current, part);
            try {
                if (fs.lstatSync(current).isSymbolicLink() || !inside(base, fs.realpathSync.native(current))) { throw new Error('Linked or redirected library paths are not supported.'); }
            } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; } }
        }
    }
    const roots = folders.map(entry => ({ id: entry.uri.toString(), name: entry.name }));
    if (Buffer.byteLength(JSON.stringify(roots)) > 8192) { throw new Error('Workspace folder names exceed the library budget. Open fewer workspace folders to browse saved files.'); }
    return { uri, root: folder.uri, workspace: folder.uri.toString(), roots };
}

/** One bounded, nonrecursive page. Never creates the library or follows linked children. */
export async function listWorkspaceFiles(kind: WorkspaceFileKind, directory: string, workspace?: string, offset = 0, preferredRoot?: string): Promise<WorkspaceFilesView> {
    const target = libraryTarget(kind, workspace, directory, preferredRoot), entries: WorkspaceFilesView['entries'][number][] = [];
    let missing = false, more: number | undefined, used = 0, consumed = 0;
    if (!Number.isInteger(offset) || offset < 0 || offset > 10000) { throw new Error('Refresh the folder to restart library paging.'); }
    if (offset === 10000) { throw new Error('The library reached its folder-entry budget. Organize files into smaller subfolders.'); }
    const add = (name: string, folder: boolean) => {
        // Names are one path component, including on providers that return malformed entries.
        if (!name || name.includes('/') || name.includes('\\') || name.includes(':') || name === '.' || name === '..' || /[\u0000-\u001f\u007f]/.test(name)) { return; }
        const relative = directory ? `${directory}/${name}` : name;
        if (relative.length > 1024 || (!folder && !(kind === 'plans' ? /\.(md|json)$/i : /\.json$/i).test(name))) { return; }
        const entry = { name, path: relative, directory: folder }, bytes = Buffer.byteLength(JSON.stringify(entry));
        if (used + bytes > 24576) { return false; }
        used += bytes; entries.push(entry);
    };
    try {
        if (target.uri.scheme === 'file') {
            const handle = fs.opendirSync(target.uri.fsPath);
            try {
                for (let at = 0; at < offset; at++) { if (!handle.readSync()) { break; } }
                let item: fs.Dirent | null;
                while ((item = handle.readSync())) {
                    if (consumed >= 100 || (!item.isSymbolicLink() && (item.isDirectory() || item.isFile()) && add(item.name, item.isDirectory()) === false)) { more = offset + consumed; break; }
                    consumed++;
                }
            } finally { handle.closeSync(); }
        } else {
            // The provider owns enumeration; display/transport remain bounded after its reply.
            await checkRemoteLibrary(target.root, kind, directory, true);
            const all = await vscode.workspace.fs.readDirectory(target.uri);
            for (const [name, type] of all.slice(offset)) {
                if (consumed >= 100 || (!(type & vscode.FileType.SymbolicLink) && add(name, type === vscode.FileType.Directory) === false)) { more = offset + consumed; break; }
                consumed++;
            }
        }
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === 'ENOENT' || (error as { code?: string }).code === 'FileNotFound') { missing = true; } else { throw error; }
    }
    if (more !== undefined && more > 10000) { throw new Error('The library exceeded its folder-entry budget. Organize files into smaller subfolders.'); }
    entries.sort((left, right) => Number(right.directory) - Number(left.directory) || left.name.localeCompare(right.name));
    return { kind, workspace: target.workspace, roots: target.roots, base: fileBase(kind), directory, entries, ...(more !== undefined ? { more } : {}), missing };
}

export async function loadWorkspaceFile(kind: WorkspaceFileKind, workspace: string, relative: string): Promise<ImportedPlan | Strategy | StudioDocument | SearchRecipe> {
    relativeFile(relative);
    const target = libraryTarget(kind, workspace, relative);
    if (!(kind === 'plans' ? /\.(md|json)$/i : /\.json$/i).test(relative)) { throw new Error('Choose a supported plan, strategy or recipe file.'); }
    if (target.uri.scheme === 'file') {
        const stat = fs.lstatSync(target.uri.fsPath);
        if (!stat.isFile() || stat.nlink !== 1) { throw new Error('Choose a regular, unlinked file.'); }
    } else { await checkRemoteLibrary(target.root, kind, relative, false); }
    const text = await readImport(target.uri, kind === 'plans' ? PLAN_LIMIT_BYTES * 2 : kind === 'strategies' ? STUDIO_LIMIT_BYTES * 2 : 8192);
    libraryTarget(kind, workspace, relative); // Recheck trust/root membership after a provider read.
    if (kind === 'plans') { return parsePlanFile(text); }
    const value = JSON.parse(text);
    if (kind === 'recipes') { const recipe = validateRecipe(value); if (!recipe) { throw new Error('Invalid AIRK search recipe.'); } return recipe; }
    return parseStrategyDocument(value);
}

export async function importRecipe(): Promise<SearchRecipe | undefined> {
    const folder = vscode.workspace.workspaceFolders?.[0]?.uri;
    const [source] = await vscode.window.showOpenDialog({ title: 'Load AIRK search recipe', canSelectMany: false, filters: { 'AIRK recipe JSON': ['json'] },
        ...(folder ? { defaultUri: vscode.Uri.joinPath(folder, '.airk', 'recipes') } : {}) }) ?? [];
    if (!source) { return undefined; }
    const recipe = validateRecipe(JSON.parse(await readImport(source, 8192)));
    if (!recipe) { throw new Error('Invalid AIRK search recipe.'); }
    return recipe;
}

/** Open rendered Markdown as an untitled document: opened, never saved. */
export async function openPlanDraft(markdown: string): Promise<{ opened: true }> {
    const document = await vscode.workspace.openTextDocument({ content: markdown, language: 'markdown' });
    await vscode.window.showTextDocument(document, { viewColumn: vscode.ViewColumn.Beside, preview: false });
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
export async function savePlanInWorkspace(markdown: string, fileName: string, authorized: () => boolean = () => vscode.workspace.isTrusted !== false,
    options: { title: string; filters: Record<string, string[]>; directory?: string; direct?: { root: string; expectedHash?: string } } = { title: 'Save AIRK plan', filters: { Markdown: ['md'] } }): Promise<PlanSaveResult> {
    const folders = (vscode.workspace.workspaceFolders ?? []).filter(folder => folder.uri.scheme === 'file' || folder.uri.scheme === 'vscode-remote');
    if (!folders.length) { return { saved: false, reason: 'Open a workspace folder to save plans in it.' }; }
    const folder = options.direct ? folders.find(entry => caseFold(path.resolve(entry.uri.fsPath)) === caseFold(path.resolve(options.direct!.root))) : folders.length === 1 ? folders[0] : (await vscode.window.showQuickPick(folders.map(entry => ({ label: entry.name, description: entry.uri.fsPath, folder: entry })),
        { title: 'Save the AIRK plan in which workspace folder?' }))?.folder;
    if (!folder) { return { saved: false, reason: 'No workspace folder was chosen.' }; }
    if (folder.uri.scheme !== 'file') { return { saved: false, reason: 'Guarded plan saving requires a local filesystem. Use Open as draft and the editor Save command for remote folders.' }; }
    const originalRoot = fs.realpathSync.native(folder.uri.fsPath);
    const rootIdentity = fs.statSync(originalRoot, { bigint: true });
    const defaultUri = vscode.Uri.joinPath(folder.uri, ...(options.directory ?? plannerDirectory()).split('/'), fileName);
    const target = options.direct ? defaultUri : await vscode.window.showSaveDialog({ title: options.title, filters: options.filters, defaultUri });
    if (!target) { return { saved: false, reason: 'Save was canceled; nothing was written.' }; }
    if (!authorized()) { return { saved: false, reason: 'Workspace authorization changed; nothing was written.' }; }
    if (target.scheme !== folder.uri.scheme || target.authority !== folder.uri.authority) { return { saved: false, reason: 'Plans can only be saved inside the chosen workspace folder.' }; }
    const fold = (value: string) => target.scheme === 'file' && process.platform === 'win32' ? value.toLowerCase() : value;
    const relative = path.posix.relative(fold(folder.uri.path), fold(target.path));
    if (!relative || relative.startsWith('..') || path.posix.isAbsolute(relative)) { return { saved: false, reason: 'Plans can only be saved inside the chosen workspace folder.' }; }
    // Show the path as the user typed it; the folded form is only for the containment check.
    const shown = target.path.slice(folder.uri.path.replace(/\/$/, '').length + 1);
    const open = vscode.workspace.textDocuments.find(document => document.uri.toString() === target.toString());
    if (open?.isDirty) { return { saved: false, reason: 'That file has unsaved edits in the editor; save or close it first. Nothing was written.' }; }
    // Capture the exact destination before confirmation. Only ENOENT means absent.
    const snapshot = () => {
        try {
            const stat = fs.lstatSync(target.fsPath, { bigint: true });
            if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1n) { throw new Error('Linked or non-file plan targets are not supported.'); }
            if (stat.size > 1048576n) { throw new Error('The existing plan target is larger than the guarded save limit (1 MiB).'); }
            return `${stat.dev}:${stat.ino}:${stat.size}:${stat.mtimeNs}:${stat.ctimeNs}:${createHash('sha256').update(fs.readFileSync(target.fsPath)).digest('hex')}`;
        } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') { return undefined; } throw error; }
    };
    // Reject redirected ancestors, including Windows junctions, before any creation/read/write.
    const parents = () => {
        const base = fs.realpathSync.native(folder.uri.fsPath), identity = fs.statSync(base, { bigint: true });
        if (caseFold(base) !== caseFold(originalRoot) || identity.dev !== rootIdentity.dev || identity.ino !== rootIdentity.ino) { throw new Error('The workspace root changed during review; nothing was written.'); }
        let current = folder.uri.fsPath;
        for (const part of path.relative(current, path.dirname(target.fsPath)).split(path.sep).filter(Boolean)) {
            current = path.join(current, part);
            try {
                const stat = fs.lstatSync(current, { bigint: true });
                if (!stat.isDirectory() || stat.isSymbolicLink() || !inside(base, fs.realpathSync.native(current))) { throw new Error('The plan directory is redirected outside the workspace or is not a directory.'); }
            } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; } }
        }
        return base;
    };
    parents();
    const before = snapshot();
    if (options.direct?.expectedHash && before?.split(':').pop() !== options.direct.expectedHash) {
        return { saved: false, reason: 'The saved Studio file changed or was removed. Load it again or choose a new save name; nothing was written.' };
    }
    if (before !== undefined && !options.direct?.expectedHash) {
        const choice = await vscode.window.showWarningMessage(`Replace ${shown}?`, { modal: true, detail: options.direct ? 'The existing file will be overwritten with this Studio draft.' : 'The existing file will be overwritten with this plan.' }, 'Replace');
        if (choice !== 'Replace') { return { saved: false, reason: 'The existing file was kept; nothing was written.' }; }
    }
    const bytes = new TextEncoder().encode(markdown);
    // No await between the final editor/authorization checks and the guarded write.
    if (!authorized() || !(vscode.workspace.workspaceFolders ?? []).some(entry => entry.uri.toString() === folder.uri.toString())) { return { saved: false, reason: 'Workspace authorization changed; nothing was written.' }; }
    if (vscode.workspace.textDocuments.some(document => caseFold(document.uri.toString()) === caseFold(target.toString()) && document.isDirty)) { return { saved: false, reason: 'That file has unsaved edits; nothing was written.' }; }
    parents();
    if (snapshot() !== before) { return { saved: false, reason: 'The plan target changed during review; nothing was written.' }; }
    fs.mkdirSync(path.dirname(target.fsPath), { recursive: true });
    parents();
    const handle = fs.openSync(target.fsPath, before === undefined ? 'wx' : fs.constants.O_RDWR | (fs.constants.O_NOFOLLOW ?? 0));
    try {
        const stat = fs.fstatSync(handle, { bigint: true });
        const current = fs.lstatSync(target.fsPath, { bigint: true });
        parents();
        if (!stat.isFile() || stat.nlink !== 1n || stat.dev !== current.dev || stat.ino !== current.ino || current.isSymbolicLink() || snapshot() !== before && before !== undefined) { throw new Error('The plan target changed before writing.'); }
        fs.writeFileSync(handle, bytes);
        fs.ftruncateSync(handle, bytes.length);
        fs.fsyncSync(handle);
    } finally { fs.closeSync(handle); }
    const hash = createHash('sha256').update(bytes).digest('hex');
    const readBack = createHash('sha256').update(await vscode.workspace.fs.readFile(target)).digest('hex');
    if (readBack !== hash) { return { saved: false, reason: 'The saved file does not match the plan that was written; check the file before handing it off.' }; }
    if (!options.direct) { await vscode.window.showTextDocument(target, { viewColumn: vscode.ViewColumn.Beside, preview: false }); }
    return { saved: true, path: shown, hash };
}

/** Explicit Studio Save targets only the selected project's fixed local library. */
export async function saveStudioInWorkspace(document: StudioDocument, name: string, root: string, expectedHash: string | undefined, authorized: () => boolean): Promise<PlanSaveResult> {
    if (!validateStudio(document) || !/^[a-z][a-z0-9-]{0,63}$/.test(name) || /^(con|prn|aux|nul|com[0-9]|lpt[0-9])$/.test(name)) { throw new Error('Choose a save name starting with a letter, using letters, numbers and hyphens (up to 64 characters).'); }
    if (!authorized()) { return { saved: false, reason: 'Workspace authorization changed; nothing was written.' }; }
    const saved = { ...document, strategy: { ...document.strategy, id: `user/${name}` } };
    if (!validateStudio(saved)) { throw new Error('The saved Studio document exceeds its schema or size limits.'); }
    return savePlanInWorkspace(JSON.stringify(saved, null, 2) + '\n', `${name}.studio.airk.json`, authorized,
        { title: 'Save AIRK Studio draft', filters: STRATEGY_FILTERS, directory: '.airk/strategies', direct: { root, expectedHash } });
}
