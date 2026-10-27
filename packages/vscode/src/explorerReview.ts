// SPDX-License-Identifier: Apache-2.0
// Review: the editor's Git changes for the selected project, joined with indexed facts. Git is the
// editor's (vscode.git); AIRK history is not used. Impact and tests are candidate checklists.
import * as path from 'node:path';
import { createHash } from 'node:crypto';
import * as vscode from 'vscode';
import type { LiveCheck } from './explorerContracts';
import { absentFromIndex, EvidenceClient, EvidenceFailure, PublicationChanged, samePublication } from './explorerEvidence';
import { explorerError } from './explorerErrors';
import { FactHandles, factList } from './explorerFacts';
import { caseFold, displayPath, inside, liveCheck } from './explorerSource';

export const REVIEW_LIMITS = { files: 50, sourceFiles: 20, declarations: 20, references: 20, testScopes: 2, inventoryPages: 2 } as const;
/** Paths treated as tests when no setting overrides it: test/tests/__tests__/spec folders and *.test.* / *.spec.* / *_test.* files. */
export const DEFAULT_TEST_PATTERN = '(^|/)(tests?|__tests__|specs?)/|[._-](test|spec)\\.[A-Za-z0-9]+$|_test\\.[A-Za-z0-9]+$';

export type ChangeKind = 'added' | 'modified' | 'deleted' | 'renamed' | 'untracked' | 'conflicted';
export interface ChangedFileView { readonly path: string; readonly change: ChangeKind; readonly staged: boolean; readonly indexed: boolean; readonly workingTree?: boolean; readonly originalPath?: string; readonly live: LiveCheck | 'deleted'; readonly language?: string }
export interface ChangesReview { readonly git: 'available' | 'unavailable' | 'no_repository'; readonly files: readonly ChangedFileView[]; readonly truncated: boolean; readonly publication?: string; readonly notes: readonly string[]; readonly offset: number; readonly total: number; readonly limit: number; readonly snapshot: string }
export interface ImpactItem { readonly name: string; readonly kind: string; readonly path: string; readonly line?: number; readonly references: number; readonly referenceFiles: readonly string[]; readonly testReferences: number; readonly truncated: boolean }
export interface ImpactReview { readonly items: readonly ImpactItem[]; readonly files: number; readonly truncated: boolean; readonly publication?: string; readonly notes: readonly string[] }
export interface TestCandidate { readonly path: string; readonly line?: number; readonly mentions: string; readonly text: string; readonly reason?: 'changed_test_file' | 'identifier_reference' }
export interface TestsReview { readonly candidates: readonly TestCandidate[]; readonly declarations: number | null; readonly pattern: string; readonly truncated: boolean; readonly publication?: string; readonly notes: readonly string[]; readonly analysis: 'complete' | 'partial' | 'unavailable'; readonly analysisError?: string }

// Minimal shape of the built-in Git extension API (vscode.git, getAPI(1)).
interface GitChange { readonly uri: vscode.Uri; readonly status: number; readonly originalUri?: vscode.Uri }
interface GitRepository { readonly rootUri: vscode.Uri; readonly state: { readonly indexChanges: readonly GitChange[]; readonly workingTreeChanges: readonly GitChange[]; readonly mergeChanges: readonly GitChange[] } }
interface GitApi { readonly repositories: readonly GitRepository[] }

/**
 * vscode.git Status → change kind. 0–4 index, 5–11 working tree (9 intent to add,
 * 10 intent to rename, 11 type changed), 12–18 merge states.
 */
export function kind(status: number): ChangeKind {
    if (status === 7) { return 'untracked'; }
    if ([1, 9].includes(status)) { return 'added'; }
    if ([2, 6].includes(status)) { return 'deleted'; }
    if ([3, 4, 10].includes(status)) { return 'renamed'; }
    if (status >= 12 && status <= 18) { return 'conflicted'; }
    return 'modified';
}

async function gitApi(): Promise<GitApi | undefined> {
    const extension = vscode.extensions.getExtension<{ getAPI(version: 1): GitApi }>('vscode.git');
    if (!extension) { return undefined; }
    try { return (extension.isActive ? extension.exports : await extension.activate()).getAPI(1); } catch { return undefined; }
}

/** Collection-relative paths of saved changes under the project root, from every repository that overlaps it. */
export async function changedPaths(root: string): Promise<{ git: ChangesReview['git']; changes: { path: string; file: string; change: ChangeKind; staged: boolean; workingTree?: boolean; originalPath?: string }[] }> {
    root = displayPath(root);
    const api = await gitApi();
    if (!api) { return { git: 'unavailable', changes: [] }; }
    const repositories = api.repositories.filter(repository => inside(displayPath(repository.rootUri.fsPath), displayPath(root)) || inside(displayPath(root), displayPath(repository.rootUri.fsPath)));
    if (!repositories.length) { return { git: 'no_repository', changes: [] }; }
    const seen = new Map<string, { path: string; file: string; change: ChangeKind; staged: boolean; workingTree?: boolean; originalPath?: string }>();
    for (const repository of repositories) {
        for (const [list, staged] of [[repository.state.mergeChanges, false], [repository.state.indexChanges, true], [repository.state.workingTreeChanges, false]] as const) {
            for (const change of list) {
                const file = displayPath(change.uri.fsPath);
                if (!inside(root, file)) { continue; }
                const relative = path.relative(root, file).split(path.sep).join('/');
                const key = caseFold(relative);
                const prior = seen.get(key);
                const old = change.originalUri?.fsPath;
                const originalPath = old && inside(root, old) ? path.relative(root, old).split(path.sep).join('/') : undefined;
                seen.set(key, { ...(prior ?? { path: relative, file, change: kind(change.status), staged }),
                    workingTree: prior?.workingTree || !staged, ...(originalPath ? { originalPath } : {}) });
            }
        }
    }
    return { git: 'available', changes: [...seen.values()].sort((a, b) => a.path.localeCompare(b.path)) };
}

const dirty = (file: string) => vscode.workspace.textDocuments.some(document => document.uri.scheme === 'file' && caseFold(document.uri.fsPath) === caseFold(file) && document.isDirty);

export interface ReviewPage { readonly offset?: number; readonly snapshot?: string; readonly publication?: string }
export function reviewChanges(context: ReviewContext, page: ReviewPage = {}): Promise<ChangesReview> {
    return consistentReview(context, scoped => changesPage(scoped, page), !page.snapshot && !page.publication);
}
async function changesPage(context: ReviewContext, page: ReviewPage): Promise<ChangesReview> {
    const { session } = context;
    const { git, changes } = await changedPaths(context.root);
    const offset = page.offset ?? 0;
    const snapshot = createHash('sha256').update(JSON.stringify([context.project, context.root, git, changes])).digest('hex');
    if (!Number.isSafeInteger(offset) || offset < 0 || offset % REVIEW_LIMITS.files !== 0) { throw new EvidenceFailure(explorerError('INVALID_MESSAGE', 'Review page offset is invalid.')); }
    if (offset > 0 && (offset >= changes.length || !page.snapshot || !page.publication) || page.snapshot && page.snapshot !== snapshot) {
        throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The Git change list changed. Refresh Review Changes to start again.'));
    }
    const notes: string[] = [];
    if (git === 'unavailable') { notes.push('The VS Code Git extension is not available in this window, so no changes can be listed.'); }
    if (git === 'no_repository') { notes.push('No Git repository contains this project root.'); }
    let publication = page.publication;
    const files: ChangedFileView[] = [];
    for (const change of changes.slice(offset, offset + REVIEW_LIMITS.files)) {
        let info: Record<string, unknown> | undefined;
        const result = await session.call('airk_file_info', { path: change.path }, { timeout: 30000 });
        publication = samePublication(publication, result);
        info = absentFromIndex(result.payload) ? undefined : result.payload;
        const hash = typeof info?.content_hash === 'string' ? info.content_hash : undefined;
        if (info && !hash) { throw new Error('AIRK returned an incomplete indexed record; indexing state is unknown.'); }
        files.push({ path: change.path, change: change.change, staged: change.staged, workingTree: change.workingTree, originalPath: change.originalPath, indexed: !!hash, live: change.change === 'deleted' ? 'deleted' : hash ? await liveCheck(change.file, hash, dirty(change.file)) : 'unchecked',
            ...(typeof info?.language === 'string' ? { language: info.language.slice(0, 40) } : {}) });
    }
    if (changes.length > REVIEW_LIMITS.files) { notes.push(`Showing ${offset + 1}–${offset + files.length} of ${changes.length} changed files. Use Previous and Next to read other pages.`); }
    if (files.some(file => file.live === 'changed_since_index' || file.live === 'not_saved')) { notes.push('Some changed files differ from their indexed bytes; facts for them describe the indexed version. Index now to refresh them.'); }
    notes.push('Git changes come from the editor against HEAD. AIRK does not keep history; facts describe the current publication only.');
    return { git, files, truncated: offset + files.length < changes.length, offset, total: changes.length, limit: REVIEW_LIMITS.files, snapshot, ...(publication ? { publication } : {}), notes };
}

interface Declaration { name: string; kind: string; path: string; line?: number }

/** Reads go through the shared fact parser, which understands both compact and rich reply shapes. */
export interface ReviewContext { readonly session: EvidenceClient; readonly handles: FactHandles; readonly project: string; readonly root: string }

/** Restart the whole read-only report once; never combine results from attempts. */
async function consistentReview<T extends { notes: readonly string[] }>(context: ReviewContext, read: (context: ReviewContext) => Promise<T>, restart = true): Promise<T> {
    const deadline = performance.now() + 90000;
    for (let attempt = 0; ; attempt++) {
        const cache = new Map<string, ReturnType<EvidenceClient['call']>>();
        const scoped = { ...context, session: { call: (tool, args, options = {}) => {
            const remaining = Math.floor(deadline - performance.now());
            if (remaining <= 0) { throw new EvidenceFailure(explorerError('REQUEST_TIMEOUT', 'Review exceeded its time budget. Narrow the Git change set and refresh.')); }
            const key = JSON.stringify([tool, args]);
            if (!cache.has(key)) { cache.set(key, context.session.call(tool, args, { ...options, timeout: Math.min(options.timeout ?? 30000, remaining) })); }
            return cache.get(key)!;
        } } as EvidenceClient };
        try {
            const result = await read(scoped);
            return attempt ? { ...result, notes: ['The index changed during the first attempt; this report was restarted against one publication.', ...result.notes] } : result;
        } catch (error) {
            if (!(error instanceof PublicationChanged) || !restart) { throw error; }
            if (attempt || performance.now() >= deadline) {
                throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The index is still changing after a bounded report restart. Wait for indexing to settle, then refresh Review. No mixed-publication report was shown.'));
            }
        }
    }
}

const CODE_PATH = /\.(?:[cm]?[jt]sx?|rs|py|go|java|kt|kts|cs|c|cc|cpp|cxx|h|hpp|rb|php|swift|scala|sc|sh|bash|zsh|ps1|lua|r|R|ex|exs|erl|hrl|hs|fs|fsx|vb|vue|svelte|sql|pl|pm|dart|jl|clj|cljs|groovy)$/;
function prioritized(paths: readonly string[]): string[] {
    const tests = testPattern();
    const rank = (file: string) => CODE_PATH.test(file) ? tests.test(file) ? 1 : 0 : 2;
    return [...paths].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
}
async function declarations(context: ReviewContext, paths: readonly string[]): Promise<{ items: Declaration[]; truncated: boolean; publication?: string; examined: number; notes: string[] }> {
    const groups: Declaration[][] = [];
    let truncated = false, publication: string | undefined;
    const selected = prioritized(paths).slice(0, REVIEW_LIMITS.sourceFiles);
    for (const file of selected) {
        const args = { path_prefix: file, limit: 100, max_bytes: 32768 };
        const result = await context.session.call('airk_symbols', args, { timeout: 30000 });
        publication = samePublication(publication, result);
        const view = factList({ tool: 'airk_symbols', args, limit: 100 }, result.payload, result.publication, context.project, context.handles);
        const declarations = view.items.filter(item => /^[A-Za-z_$][\w$]{0,127}$/.test(item.name) && item.path === file);
        // Prefer written public/function/type signatures, then sample across files rather than
        // letting local variables in the first file consume the entire declaration budget.
        const rank = (item: typeof declarations[number]) => /^(?:export|pub|public|protected|async|function|class|interface|type|struct|enum|trait|fn|def)\b/.test(item.signature ?? '') ? 0 : 1;
        declarations.sort((a, b) => rank(a) - rank(b) || (a.startLine ?? 0) - (b.startLine ?? 0));
        groups.push([...new Map(declarations.map(item => [`${item.path}:${item.startLine}:${item.name}`, { name: item.name, kind: item.kind, path: item.path, ...(item.startLine ? { line: item.startLine } : {}) }])).values()]);
        if (view.truncated || view.more) { truncated = true; }
    }
    const items: Declaration[] = [];
    for (let row = 0; items.length < REVIEW_LIMITS.declarations && groups.some(group => group[row]); row++) {
        for (const group of groups) { if (group[row] && items.length < REVIEW_LIMITS.declarations) { items.push(group[row]); } }
    }
    const total = groups.reduce((count, group) => count + group.length, 0);
    truncated ||= paths.length > selected.length || total > items.length;
    const notes = [`Examined ${selected.length} of ${paths.length} changed/current-or-old paths, prioritizing source files; sampled ${items.length} of ${total} returned declarations across files.`];
    if (truncated) { notes.push('Review is bounded: some paths, declarations or symbol pages were omitted. An empty or partial checklist is not proof of no impact.'); }
    if (!items.length && selected.length) { notes.push('No usable indexed declarations were returned for the inspected paths. Check index freshness, extraction detail and parser support; changed tests are still listed separately.'); }
    return { items, truncated, publication, examined: selected.length, notes };
}

async function references(context: ReviewContext, name: string, publication: string | undefined, prefix?: string): Promise<{ hits: { path: string; line: number; text: string }[]; truncated: boolean }> {
    const args = { name, ...(prefix ? { path_prefix: prefix } : {}), limit: REVIEW_LIMITS.references, max_bytes: 16384 };
    const result = await context.session.call('airk_references', args, { timeout: 30000 });
    samePublication(publication, result);
    const view = factList({ tool: 'airk_references', args, limit: REVIEW_LIMITS.references }, result.payload, result.publication, context.project, context.handles);
    const hits = view.items.flatMap(item => item.startLine && (!prefix || item.path.startsWith(prefix)) ? [{ path: item.path, line: item.startLine, text: (item.detail ?? '').slice(0, 300) }] : []);
    return { hits, truncated: view.truncated || hits.length >= REVIEW_LIMITS.references };
}

function testScope(file: string): string {
    const folder = /(^|\/)(?:tests?|__tests__|specs?)\//.exec(file);
    return folder ? file.slice(0, folder.index + folder[0].length) : file.includes('/') ? file.slice(0, file.lastIndexOf('/') + 1) : '';
}
async function testScopes(context: ReviewContext, paths: readonly string[], pattern: RegExp, publication: string | undefined): Promise<{ prefixes: string[]; publication?: string; truncated: boolean; notes: string[] }> {
    const known = new Set(paths.filter(file => pattern.test(file)).map(testScope));
    let truncated = false;
    if (!known.size && paths.length) {
        const availableModules = [...new Set(prioritized(paths).filter(file => CODE_PATH.test(file)).map(file => {
            const source = /(^|\/)src\//.exec(file);
            return source ? file.slice(0, source.index + (source.index ? 1 : 0)) : file.includes('/') ? file.slice(0, file.lastIndexOf('/') + 1) : '';
        }))];
        const modules = availableModules.slice(0, REVIEW_LIMITS.testScopes);
        truncated ||= availableModules.length > modules.length;
        for (const prefix of modules) {
            let cursor: string | undefined;
            for (let page = 0; page < REVIEW_LIMITS.inventoryPages; page++) {
                const result = await context.session.call('airk_list_files', { prefix, limit: 100, max_bytes: 32768, ...(cursor ? { cursor } : {}) }, { timeout: 30000 });
                publication = samePublication(publication, result);
                const indexed = Array.isArray(result.payload.paths) ? result.payload.paths : Array.isArray(result.payload.files) ? result.payload.files.map(raw => (raw as { path?: string }).path) : [];
                for (const file of indexed) { if (typeof file === 'string' && !file.startsWith('/') && !file.includes('\\') && !file.includes(':') && !file.split('/').includes('..') && pattern.test(file)) { known.add(testScope(file)); } }
                cursor = typeof result.payload.cursor === 'string' && result.payload.cursor.length <= 8192 ? result.payload.cursor : undefined;
                if (!cursor) { truncated ||= result.payload.truncated === true; break; }
                if (page === REVIEW_LIMITS.inventoryPages - 1) { truncated = true; }
            }
        }
    }
    const all = [...known].sort();
    const independent = all.filter(prefix => !all.some(other => other !== prefix && prefix.startsWith(other)));
    const prefixes = independent.slice(0, REVIEW_LIMITS.testScopes);
    truncated ||= independent.length > prefixes.length;
    return { prefixes, publication, truncated, notes: [prefixes.length ? `Test references are searched inside ${prefixes.map(prefix => prefix || '(project root)').join(', ')} before applying hit limits.` : 'No test directory was found in the bounded inventory. Changed test files remain visible; this does not establish that tests are absent.', ...(truncated ? ['Test-scope discovery was clipped; other directories may contain relevant tests.'] : [])] };
}

function testPattern(): RegExp {
    const configured = vscode.workspace.getConfiguration('airk').get<string>('explorer.review.testPathPattern', DEFAULT_TEST_PATTERN);
    try { return new RegExp(typeof configured === 'string' && configured.trim() && configured.length <= 512 ? configured : DEFAULT_TEST_PATTERN); }
    catch { return new RegExp(DEFAULT_TEST_PATTERN); }
}

const changedSource = (changes: { path: string; change: ChangeKind }[]) => [...new Set(changes.flatMap(change => [change.path, ...('originalPath' in change && typeof change.originalPath === 'string' ? [change.originalPath] : [])]))];

export function reviewImpact(context: ReviewContext): Promise<ImpactReview> {
    return consistentReview(context, impactReport);
}
async function impactReport(context: ReviewContext): Promise<ImpactReview> {
    const { git, changes } = await changedPaths(context.root);
    const pattern = testPattern(), paths = changedSource(changes);
    const found = await declarations(context, paths);
    const scopes = await testScopes(context, paths, pattern, found.publication);
    const publication = scopes.publication ?? found.publication;
    const items: ImpactItem[] = [];
    for (const declaration of found.items) {
        const refs = await references(context, declaration.name, publication);
        const hits = [...refs.hits]; let clipped = refs.truncated;
        for (const prefix of scopes.prefixes) {
            const tests = await references(context, declaration.name, publication, prefix);
            hits.push(...tests.hits.filter(hit => pattern.test(hit.path))); clipped ||= tests.truncated;
        }
        const unique = [...new Map(hits.map(hit => [`${hit.path}:${hit.line}`, hit])).values()];
        const files = [...new Set(unique.map(hit => hit.path))];
        items.push({ ...declaration, references: unique.length, referenceFiles: files.slice(0, 5), testReferences: unique.filter(hit => pattern.test(hit.path)).length, truncated: clipped || files.length > 5 });
    }
    return { items, files: changes.length, truncated: found.truncated || scopes.truncated || items.some(item => item.truncated), ...(publication ? { publication } : {}),
        notes: ['A checklist, not a verdict: references are exact-case textual occurrences in the index, not resolved callers. Declarations describe changed files, not a line-diff proof that each declaration changed. Old paths already removed from the index cannot be recovered.', ...gitNotes(git, changes.length), ...found.notes, ...scopes.notes] };
}

function gitNotes(git: ChangesReview['git'], changes: number): string[] {
    return git === 'unavailable' ? ['The VS Code Git extension is unavailable; no changed-file review could be performed.']
        : git === 'no_repository' ? ['No Git repository contains the selected project.'] : changes ? [] : ['No uncommitted Git changes were found. This review does not compare earlier commits.'];
}

export async function reviewTests(context: ReviewContext): Promise<TestsReview> {
    try { return await consistentReview(context, testsReport); }
    catch (error) {
        if (error instanceof EvidenceFailure && ['REQUEST_SUPERSEDED', 'STALE_BINDING', 'WORKSPACE_UNTRUSTED', 'NO_PROJECT'].includes(error.error.code)) { throw error; }
        const { git, changes } = await changedPaths(context.root), pattern = testPattern();
        const tests = changes.filter(change => pattern.test(change.path));
        return { candidates: tests.slice(0, 100).map(change => ({ path: change.path, mentions: '', text: '', reason: 'changed_test_file' })),
            declarations: null, pattern: pattern.source, truncated: true, analysis: 'unavailable', analysisError: error instanceof Error ? error.message : String(error),
            notes: ['Indexed test analysis could not complete. Only test files reported changed by Git are shown; no indexed references, execution or coverage are claimed.',
                ...(tests.length > 100 ? [`Showing 100 of ${tests.length} changed test files. Review Changes can page through all changed files.`] : []), ...gitNotes(git, changes.length)] };
    }
}
async function testsReport(context: ReviewContext): Promise<TestsReview> {
    const { git, changes } = await changedPaths(context.root);
    const pattern = testPattern(), paths = changedSource(changes);
    const found = await declarations(context, paths);
    const scopes = await testScopes(context, paths, pattern, found.publication);
    const publication = scopes.publication ?? found.publication;
    const candidates: TestCandidate[] = changes.filter(change => pattern.test(change.path)).map(change => ({ path: change.path, mentions: '', text: '', reason: 'changed_test_file' }));
    let truncated = found.truncated || scopes.truncated;
    for (const declaration of found.items) {
        const prefixes: (string | undefined)[] = scopes.prefixes.length ? scopes.prefixes : [undefined];
        for (const prefix of prefixes) {
            const refs = await references(context, declaration.name, publication, prefix);
            truncated ||= refs.truncated;
            for (const hit of refs.hits.filter(entry => pattern.test(entry.path))) { candidates.push({ ...hit, mentions: declaration.name, reason: 'identifier_reference' }); }
        }
    }
    const unique = [...new Map(candidates.map(candidate => [`${candidate.path}:${candidate.line ?? 0}:${candidate.mentions}`, candidate])).values()];
    truncated ||= unique.length > 100;
    return { candidates: unique.sort((a, b) => Number(b.reason === 'changed_test_file') - Number(a.reason === 'changed_test_file') || a.path.localeCompare(b.path) || (a.line ?? 0) - (b.line ?? 0)).slice(0, 100), declarations: found.items.length, pattern: pattern.source, truncated, publication, analysis: truncated ? 'partial' : 'complete',
        notes: ['Changed test files and indexed textual-reference candidates. They were not executed, and this is not coverage. Changed-file entries come from Git and do not claim indexed evidence or a fabricated source line.',
            ...(unique.length > 100 ? [`Showing 100 of ${unique.length} candidate entries, with changed test files first. Review Changes can page through all changed files.`] : []), ...gitNotes(git, changes.length), ...found.notes, ...scopes.notes] };
}
