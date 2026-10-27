// SPDX-License-Identifier: Apache-2.0
// Review: the editor's Git changes for the selected project, joined with indexed facts. Git is the
// editor's (vscode.git); AIRK history is not used. Impact and tests are candidate checklists.
import * as path from 'node:path';
import * as vscode from 'vscode';
import type { LiveCheck } from './explorerContracts';
import { EvidenceClient } from './explorerEvidence';
import { FactHandles, factList } from './explorerFacts';
import { caseFold, inside, liveCheck } from './explorerSource';

export const REVIEW_LIMITS = { files: 50, declarations: 20, references: 20 } as const;
/** Paths treated as tests when no setting overrides it: test/tests/__tests__/spec folders and *.test.* / *.spec.* / *_test.* files. */
export const DEFAULT_TEST_PATTERN = '(^|/)(tests?|__tests__|specs?)/|[._-](test|spec)\\.[A-Za-z0-9]+$|_test\\.[A-Za-z0-9]+$';

export type ChangeKind = 'added' | 'modified' | 'deleted' | 'renamed' | 'untracked' | 'conflicted';
export interface ChangedFileView { readonly path: string; readonly change: ChangeKind; readonly staged: boolean; readonly indexed: boolean; readonly live: LiveCheck | 'deleted'; readonly language?: string }
export interface ChangesReview { readonly git: 'available' | 'unavailable' | 'no_repository'; readonly files: readonly ChangedFileView[]; readonly truncated: boolean; readonly publication?: string; readonly notes: readonly string[] }
export interface ImpactItem { readonly name: string; readonly kind: string; readonly path: string; readonly line?: number; readonly references: number; readonly referenceFiles: readonly string[]; readonly testReferences: number; readonly truncated: boolean }
export interface ImpactReview { readonly items: readonly ImpactItem[]; readonly files: number; readonly truncated: boolean; readonly publication?: string; readonly notes: readonly string[] }
export interface TestCandidate { readonly path: string; readonly line: number; readonly mentions: string; readonly text: string }
export interface TestsReview { readonly candidates: readonly TestCandidate[]; readonly declarations: number; readonly pattern: string; readonly truncated: boolean; readonly notes: readonly string[] }

// Minimal shape of the built-in Git extension API (vscode.git, getAPI(1)).
interface GitChange { readonly uri: vscode.Uri; readonly status: number }
interface GitRepository { readonly rootUri: vscode.Uri; readonly state: { readonly indexChanges: readonly GitChange[]; readonly workingTreeChanges: readonly GitChange[]; readonly mergeChanges: readonly GitChange[] } }
interface GitApi { readonly repositories: readonly GitRepository[] }

/** Git API status numbers → change kinds (index and working tree). */
function kind(status: number): ChangeKind {
    if ([1, 7, 9].includes(status)) { return status === 7 ? 'untracked' : 'added'; }
    if ([2, 6].includes(status)) { return 'deleted'; }
    if ([3, 4].includes(status)) { return 'renamed'; }
    if (status >= 10 && status <= 16) { return 'conflicted'; }
    return 'modified';
}

async function gitApi(): Promise<GitApi | undefined> {
    const extension = vscode.extensions.getExtension<{ getAPI(version: 1): GitApi }>('vscode.git');
    if (!extension) { return undefined; }
    try { return (extension.isActive ? extension.exports : await extension.activate()).getAPI(1); } catch { return undefined; }
}

/** Collection-relative paths of saved changes under the project root, from every repository that overlaps it. */
export async function changedPaths(root: string): Promise<{ git: ChangesReview['git']; changes: { path: string; file: string; change: ChangeKind; staged: boolean }[] }> {
    const api = await gitApi();
    if (!api) { return { git: 'unavailable', changes: [] }; }
    const repositories = api.repositories.filter(repository => inside(repository.rootUri.fsPath, root) || inside(root, repository.rootUri.fsPath));
    if (!repositories.length) { return { git: 'no_repository', changes: [] }; }
    const seen = new Map<string, { path: string; file: string; change: ChangeKind; staged: boolean }>();
    for (const repository of repositories) {
        for (const [list, staged] of [[repository.state.mergeChanges, false], [repository.state.indexChanges, true], [repository.state.workingTreeChanges, false]] as const) {
            for (const change of list) {
                const file = change.uri.fsPath;
                if (!inside(root, file)) { continue; }
                const relative = path.relative(root, file).split(path.sep).join('/');
                const key = caseFold(relative);
                if (!seen.has(key)) { seen.set(key, { path: relative, file, change: kind(change.status), staged }); }
            }
        }
    }
    return { git: 'available', changes: [...seen.values()].sort((a, b) => a.path.localeCompare(b.path)) };
}

const dirty = (file: string) => vscode.workspace.textDocuments.some(document => document.uri.scheme === 'file' && caseFold(document.uri.fsPath) === caseFold(file) && document.isDirty);

export async function reviewChanges(context: ReviewContext): Promise<ChangesReview> {
    const { session } = context;
    const { git, changes } = await changedPaths(context.root);
    const notes: string[] = [];
    if (git === 'unavailable') { notes.push('The VS Code Git extension is not available in this window, so no changes can be listed.'); }
    if (git === 'no_repository') { notes.push('No Git repository contains this project root.'); }
    let publication: string | undefined;
    const files: ChangedFileView[] = [];
    for (const change of changes.slice(0, REVIEW_LIMITS.files)) {
        if (change.change === 'deleted') { files.push({ path: change.path, change: change.change, staged: change.staged, indexed: false, live: 'deleted' }); continue; }
        let info: Record<string, unknown> | undefined;
        try { const result = await session.call('airk_file_info', { path: change.path }, { timeout: 30000 }); info = result.payload; publication ??= result.publication; }
        catch { info = undefined; }
        const hash = typeof info?.content_hash === 'string' ? info.content_hash : undefined;
        files.push({ path: change.path, change: change.change, staged: change.staged, indexed: !!hash, live: hash ? await liveCheck(change.file, hash, dirty(change.file)) : 'unchecked',
            ...(typeof info?.language === 'string' ? { language: info.language.slice(0, 40) } : {}) });
    }
    if (changes.length > REVIEW_LIMITS.files) { notes.push(`Showing the first ${REVIEW_LIMITS.files} of ${changes.length} changed files.`); }
    if (files.some(file => file.live === 'changed_since_index' || file.live === 'not_saved')) { notes.push('Some changed files differ from their indexed bytes; facts for them describe the indexed version. Index now to refresh them.'); }
    notes.push('Git changes come from the editor against HEAD. AIRK does not keep history; facts describe the current publication only.');
    return { git, files, truncated: changes.length > REVIEW_LIMITS.files, ...(publication ? { publication } : {}), notes };
}

interface Declaration { name: string; kind: string; path: string; line?: number }

/** Reads go through the shared fact parser, which understands both compact and rich reply shapes. */
export interface ReviewContext { readonly session: EvidenceClient; readonly handles: FactHandles; readonly project: string; readonly root: string }

async function declarations(context: ReviewContext, paths: readonly string[]): Promise<{ items: Declaration[]; truncated: boolean; publication?: string }> {
    const items: Declaration[] = [];
    let truncated = false, publication: string | undefined;
    for (const file of paths.slice(0, 10)) {
        const args = { path_prefix: file, limit: 50, max_bytes: 16384 };
        const result = await context.session.call('airk_symbols', args, { timeout: 30000 });
        publication ??= result.publication;
        const view = factList({ tool: 'airk_symbols', args, limit: 50 }, result.payload, result.publication, context.project, context.handles);
        for (const item of view.items) {
            if (!/^[A-Za-z_$][\w$]{1,127}$/.test(item.name) || item.path !== file) { continue; }
            items.push({ name: item.name, kind: item.kind, path: item.path, ...(item.startLine ? { line: item.startLine } : {}) });
        }
        if (view.truncated || view.more) { truncated = true; }
    }
    if (paths.length > 10) { truncated = true; }
    const unique = [...new Map(items.map(item => [item.name, item])).values()];
    return { items: unique.slice(0, REVIEW_LIMITS.declarations), truncated: truncated || unique.length > REVIEW_LIMITS.declarations, ...(publication ? { publication } : {}) };
}

async function references(context: ReviewContext, name: string): Promise<{ hits: { path: string; line: number; text: string }[]; truncated: boolean }> {
    const args = { name, limit: REVIEW_LIMITS.references, max_bytes: 16384 };
    const result = await context.session.call('airk_references', args, { timeout: 30000 });
    const view = factList({ tool: 'airk_references', args, limit: REVIEW_LIMITS.references }, result.payload, result.publication, context.project, context.handles);
    const hits = view.items.flatMap(item => item.startLine ? [{ path: item.path, line: item.startLine, text: (item.detail ?? '').slice(0, 300) }] : []);
    return { hits, truncated: view.truncated || hits.length >= REVIEW_LIMITS.references };
}

function testPattern(): RegExp {
    const configured = vscode.workspace.getConfiguration('airk').get<string>('explorer.review.testPathPattern', DEFAULT_TEST_PATTERN);
    try { return new RegExp(typeof configured === 'string' && configured.trim() && configured.length <= 512 ? configured : DEFAULT_TEST_PATTERN); }
    catch { return new RegExp(DEFAULT_TEST_PATTERN); }
}

const changedSource = (changes: { path: string; change: ChangeKind }[]) => changes.filter(change => change.change !== 'deleted').map(change => change.path);

export async function reviewImpact(context: ReviewContext): Promise<ImpactReview> {
    const { changes } = await changedPaths(context.root);
    const pattern = testPattern();
    const found = await declarations(context, changedSource(changes));
    const items: ImpactItem[] = [];
    for (const declaration of found.items) {
        const refs = await references(context, declaration.name);
        const files = [...new Set(refs.hits.map(hit => hit.path))];
        items.push({ ...declaration, references: refs.hits.length, referenceFiles: files.slice(0, 5), testReferences: refs.hits.filter(hit => pattern.test(hit.path)).length, truncated: refs.truncated });
    }
    return { items, files: changes.length, truncated: found.truncated, ...(found.publication ? { publication: found.publication } : {}),
        notes: ['A checklist, not a verdict: references are exact-case textual occurrences in the index, not resolved callers. Declarations come from the indexed version of each changed file.'] };
}

export async function reviewTests(context: ReviewContext): Promise<TestsReview> {
    const { changes } = await changedPaths(context.root);
    const pattern = testPattern();
    const found = await declarations(context, changedSource(changes));
    const candidates: TestCandidate[] = [];
    let truncated = found.truncated;
    for (const declaration of found.items) {
        const refs = await references(context, declaration.name);
        truncated ||= refs.truncated;
        for (const hit of refs.hits.filter(entry => pattern.test(entry.path))) { candidates.push({ path: hit.path, line: hit.line, mentions: declaration.name, text: hit.text }); }
    }
    return { candidates: candidates.sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line).slice(0, 100), declarations: found.items.length, pattern: pattern.source, truncated,
        notes: ['Declared test candidates: test-path lines that mention a changed declaration. They were not executed, and this is not coverage.'] };
}
