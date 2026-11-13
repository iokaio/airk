// SPDX-License-Identifier: Apache-2.0
//! The window search scope (U77, plan §8.1 "Window search scope is chosen
//! among existing indexes through the session"; contracts §2.5 W4, §3.7).
//! Choices are read read-only through the session's metadata relay
//! (`catalog_page`, kinds `indexes` and `projects`) at one catalog revision;
//! a choice is granted with `scope_select` and dropped with `scope_release`.
//! (U104) The scope is a union: every member of the checked index sets plus
//! every checked index. Applying an index that is in no index set first
//! creates a visible index set named after it (`airkctl library set-create
//! --index`), and the checked indexes can be saved as a new named index set;
//! nothing here creates an index or changes an agent grant. The last choice is remembered per
//! workspace and applied again after a reconnect only after the catalog is
//! read again at its current revision; a stale revision is never applied
//! blindly. A read refused `scoped_read_unavailable` (after the session's
//! one Q4 re-select) marks the scope out of date for the user to reselect.
//!
//! (U103) Without a chosen scope, the window's own index is read under an
//! automatic scope: the catalog index whose backing binding is the workspace
//! project's, with the index sets that hold it. An index in no index set is
//! not readable (selection-authority §3.5 Q1); it is never read through the
//! refused binding route. Applying it in Indexing › Search scope (U104) or AIRK
//! Admin's "Make readable" gives it an index set. A choice the user applied or
//! cleared is never overridden. Picking indexes alone reads them through every
//! index set that holds them.
import { createHash } from 'node:crypto';
import type { SessionOutcome, WindowSelection } from '../../generated/i2';
import { catalogHex, catalogId, catalogItems, catalogObject, catalogRevision, type CatalogObject } from '../../indexingCatalog';
import type { LaunchSpec } from '../../model';
import type { ReadRouteView, SearchScopeSummary } from '../../explorerContracts';
import type { AgentDefaultScope } from '../agents/index';

export const WINDOW_SCOPE_CAPABILITY = 'window-scope/1';
/** workspaceState key of the remembered choice (selection only; never a grant or lease). */
export const WINDOW_SCOPE_KEY = 'airk.windowScope.selection';
/** Native page budgets (CatalogPageRequest; MAX_SELECTION = 32, MAX_PAGE_BYTES = 48 KiB). */
export const SCOPE_PAGE_LIMIT = 16;
export const SCOPE_PAGE_BYTES = 48 * 1024;
export const SCOPE_MAX_SELECTION = 32;
const MEMBERSHIP_PAGES = 16, DISPLAY_LIMIT = 512;
/** Refusals of a window-scoped read that mean the user must choose again. */
export const WINDOW_SCOPE_REFUSALS: readonly string[] = ['scoped_read_unavailable', 'window_scope_not_granted'];
export const SCOPE_OUT_OF_DATE = 'Search scope out of date — reselect.';
/** workspaceState key: the user cleared the search scope; no automatic scope replaces that choice. */
export const WINDOW_SCOPE_CLEARED_KEY = 'airk.windowScope.userCleared';
/** Pages read to find the workspace index (SCOPE_PAGE_LIMIT rows each). */
const INDEX_PAGES = 32;

export class WindowScopeError extends Error {
    constructor(readonly code: string, message: string, readonly nativeResponse?: Record<string, unknown>) { super(message); this.name = 'WindowScopeError'; }
}
const refusalMessage = (code: string) => code === 'scoped_read_unavailable' ? SCOPE_OUT_OF_DATE
    : code === 'window_scope_not_granted' ? 'This window has no search scope. Choose one again.'
    : `The AIRK Service refused the search scope (${code}).`;

/** The window-plane and relay methods of `AirkctlSession` this module uses. */
export interface ScopeSession {
    control(method: unknown): Promise<Record<string, unknown>>;
    scopeSelect(selection: WindowSelection): Promise<SessionOutcome>;
    scopeRelease(): Promise<SessionOutcome>;
}
/** (U104) Creates a visible index set with full-source memberships (`airkctl library set-create --index … --yes`).
 * (U105) Sets or clears an index's or index set's description (`airkctl library describe … --yes`). */
export interface ScopeLibrary {
    createSet(name: string, index_ids: readonly string[]): Promise<Record<string, unknown>>;
    describe?(kind: DescriptionKind, id: string, text: string | undefined): Promise<Record<string, unknown>>;
}
/** (U105) What a description belongs to: an index (`--index`) or an index set (`--set`). */
export type DescriptionKind = 'index' | 'set';
/** (U105) The description bound: 1 KiB of UTF-8, newlines allowed, no other control characters. */
export const DESCRIPTION_MAX_BYTES = 1024;
/** The exact `library describe` argv: `text` sets the description, undefined clears it. */
export function describeArgv(kind: DescriptionKind, id: string, text: string | undefined): string[] {
    return ['library', 'describe', kind === 'index' ? '--index' : '--set', id, ...(text === undefined ? ['--clear'] : [`--text=${text}`]), '--yes'];
}
/** The structural `AirkctlSession.invoke` the library command runs through. */
export interface LibraryInvoker {
    invoke(argv: readonly string[], options?: { deadlineMs?: number }): Promise<{ exitCode: number; class: string; stdout: string; stderr: string; error?: { code: string; message: string } }>;
}
/** The exact `library set-create` argv for one new index set. */
export function setCreateArgv(name: string, index_ids: readonly string[]): string[] {
    return ['library', 'set-create', '--name', name, ...index_ids.flatMap(id => ['--index', id]), '--yes'];
}
/** Run `library set-create` and `library describe`; every printed control response line is kept, a refusal keeps its code. */
export function libraryCommands(session: LibraryInvoker): ScopeLibrary {
    const run = async (argv: string[], fallback: string, failed: (reason: string) => string) => {
        const result = await session.invoke(argv, { deadlineMs: 60_000 });
        const lines = result.stdout.split('\n').filter(line => line.trim()).map(line => { try { return JSON.parse(line) as unknown; } catch { return line; } });
        const outcome = { argv, class: result.class, exitCode: result.exitCode, lines, ...(result.stderr ? { stderr: result.stderr } : {}), ...(result.error ? { error: result.error } : {}) };
        if (result.class !== 'success') {
            const code = result.error?.code && /^[a-z0-9_]{1,128}$/.test(result.error.code) ? result.error.code : fallback;
            throw new WindowScopeError(code, failed(result.error?.message ?? result.class), { status: 'refused', code, ...outcome });
        }
        return outcome;
    };
    return {
        createSet: (name, index_ids) => run(setCreateArgv(name, index_ids), 'index_set_create_failed', reason => `The index set "${name}" was not created: ${reason}`),
        describe: (kind, id, text) => run(describeArgv(kind, id, text), 'description_change_failed',
            reason => `The description was not ${text === undefined ? 'cleared' : 'saved'}: ${reason}`),
    };
}
export interface ScopeStore { get<T>(key: string): T | undefined; update(key: string, value: unknown): PromiseLike<void> }
/** What a window-scope launch spec is built from (the verified installation). */
export interface ScopeBase { readonly command: string; readonly installDir: string; readonly root: string }

/** The choices the Indexing page renders (`CatalogView.searchScope`). */
export interface SearchScopeChoices {
    indexes: CatalogObject[]; projects: CatalogObject[]; selectedIndexIds: string[]; selectedProjectIds: string[];
    active: boolean; outOfDate?: boolean; namespace?: string; revision?: number;
    /** (U104) The applied scope is the automatic one (U103); applying it makes it the user's choice. */
    automatic?: boolean;
    indexContinuation?: CatalogObject | null; projectContinuation?: CatalogObject | null;
    indexResponses?: CatalogObject[]; projectResponses?: CatalogObject[]; responses: CatalogObject[];
    /** Index → the index sets that grant it read authority, from every membership at `revision` (U103).
     * Absent or incomplete: the Indexing page leaves validation to the service. */
    indexSets?: Record<string, string[]>; indexSetsComplete?: boolean; indexSetsError?: string; membershipResponses?: CatalogObject[];
}
interface Active {
    readonly selection: WindowSelection; readonly scopeIdentity: string; readonly catalogRevision: number;
    readonly expiresAtMs: number; readonly label: string; readonly explicit: boolean;
    /** What the user checked (the Indexing page shows only these); the granted selection may add sets or members. */
    readonly chosenIndexIds: readonly string[]; readonly chosenProjectIds: readonly string[];
    /** Granted automatically for the workspace index (never remembered as the user's choice). */
    readonly auto?: { readonly binding: string };
    /** (U105) The default scope a following open agent connection receives for this grant. */
    readonly agent: AgentDefaultScope;
}
/** (U105) A change of the scope that following open agent connections receive. `scope` null: the user cleared
 * the window scope. `user`: the user applied or cleared it (an automatic or restored grant is not). */
export interface AgentScopeChange { readonly scope: AgentDefaultScope | null; readonly user: boolean }
/** `resolved`: the index sets were derived from picked indexes, so they are derived again on restore.
 * `union` (U104): `project_ids`/`index_ids` are the checked sets and indexes, granted as a union;
 * without it (earlier records, Recommendations and agent drafts) the checked indexes narrow the sets. */
interface Remembered { namespace: string; project_ids: string[]; index_ids: string[]; explicit: boolean; resolved?: boolean; union?: boolean }
interface GrantOptions { readonly chosenProjectIds: readonly string[]; readonly chosenIndexIds: readonly string[]; readonly label: string;
    readonly remember?: Omit<Remembered, 'namespace'>; readonly auto?: { readonly binding: string };
    readonly agent: AgentDefaultScope; readonly user: boolean }
type Mode = 'union' | 'exact';
const sameSet = (left: readonly string[], right: readonly string[]) => left.length === right.length && left.every(item => right.includes(item));

function ids(value: unknown, prefix: 'idx_' | 'prj_'): string[] {
    if (value === undefined || value === null) { return []; }
    if (!Array.isArray(value)) { throw new WindowScopeError('window_scope_selection_invalid', 'The search scope selection is invalid.'); }
    const out = value.map(item => { try { return catalogId(item, prefix); } catch { throw new WindowScopeError('window_scope_selection_invalid', 'The search scope selection is invalid.'); } });
    if (new Set(out).size !== out.length || out.length > SCOPE_MAX_SELECTION) {
        throw new WindowScopeError('window_scope_selection_invalid', `Choose at most ${SCOPE_MAX_SELECTION} distinct indexes and ${SCOPE_MAX_SELECTION} distinct index sets.`);
    }
    return out;
}
function remembered(value: unknown): Remembered | undefined {
    const item = value as Partial<Remembered> | undefined;
    try {
        if (!item || typeof item !== 'object' || typeof item.explicit !== 'boolean') { return undefined; }
        return { namespace: catalogHex(item.namespace), project_ids: ids(item.project_ids, 'prj_'), index_ids: ids(item.index_ids, 'idx_'), explicit: item.explicit,
            ...(item.resolved === true ? { resolved: true } : {}), ...(item.union === true ? { union: true } : {}) };
    } catch { return undefined; }
}
/** A stable 64-hex key of one selection (Workbench history), independent of the catalog revision. */
export function windowScopeKey(selection: Pick<WindowSelection, 'namespace' | 'index_ids' | 'project_ids'>): string {
    return createHash('sha256').update(`airk/window-scope/1\u0000${JSON.stringify([selection.namespace, [...selection.project_ids].sort(), [...selection.index_ids].sort()])}`).digest('hex');
}

/** (U105) A stable 64-hex key of one explicit open selection (Workbench history); names are kept as given. */
export function openSelectionKey(selection: { readonly indexes: readonly string[]; readonly sets: readonly string[] }): string {
    return createHash('sha256').update(`airk/workbench-open-selection/1\u0000${JSON.stringify([[...selection.indexes].sort(), [...selection.sets].sort()])}`).digest('hex');
}
/** (U105) The Workbench entry reading explicitly named indexes and index sets (`--index`/`--set`):
 * no project, session or window grant is named; `service.project` only carries the history key. */
export function openSelectionSpec(base: ScopeBase, selection: { readonly indexes: readonly string[]; readonly sets: readonly string[] }, label: string): LaunchSpec {
    const key = openSelectionKey(selection);
    return { command: base.command, root: base.root, dataDir: '', collection: 'open-selection', name: `Named: ${label}`,
        service: { installDir: base.installDir, project: key }, openSelection: { key, label, indexes: [...selection.indexes], sets: [...selection.sets] } };
}

export class WindowScope {
    private choices?: SearchScopeChoices;
    private current?: Active;
    private stale = false;
    private reads = 0;
    private restoring?: Promise<void>;
    private route?: ReadRouteView;
    private resolving?: { binding: string; work: Promise<void> };
    /** The latest automatic resolution; an older one never grants (U103). */
    private defaultTicket = 0;
    /** The binding the current route describes. */
    private routeBinding?: string;
    private readonly listeners = new Set<() => void>();
    private readonly agentListeners = new Set<(change: AgentScopeChange) => void>();
    /** Changes on every published change (the Explorer's delivery tag). */
    generation = 0;

    constructor(private readonly session: ScopeSession, private readonly capabilities: () => readonly string[] | undefined,
        private readonly store: ScopeStore, private readonly base: () => ScopeBase | undefined, private readonly log: (line: string) => void = () => {},
        private readonly library?: ScopeLibrary) {}

    available(): boolean { return this.capabilities()?.includes(WINDOW_SCOPE_CAPABILITY) === true; }
    onDidChange(listener: () => void): { dispose(): void } { this.listeners.add(listener); return { dispose: () => { this.listeners.delete(listener); } }; }
    private changed(): void { this.generation++; for (const listener of [...this.listeners]) { try { listener(); } catch { /* Listener failures stay local. */ } } }
    /** (U105) After every granted scope (user, automatic or restored) and every user clear. */
    onDidChangeAgentScope(listener: (change: AgentScopeChange) => void): { dispose(): void } {
        this.agentListeners.add(listener); return { dispose: () => { this.agentListeners.delete(listener); } };
    }
    private agentScopeChanged(change: AgentScopeChange): void {
        for (const listener of [...this.agentListeners]) { try { listener(change); } catch { /* Listener failures stay local. */ } }
    }
    /** (U105) The default scope for open agent connections: the user's checked index sets and indexes (union),
     * the granted indexes of an exact choice, or the window's index for the automatic scope; undefined without a scope. */
    agentDefaultScope(): AgentDefaultScope | undefined {
        const current = this.active ? this.current : undefined;
        return current ? { index_ids: [...current.agent.index_ids], project_ids: [...current.agent.project_ids] } : undefined;
    }
    /** (U105) Name and description of a loaded choice (undefined when not loaded). */
    choiceInfo(kind: DescriptionKind, id: string): { name?: string; description?: string } | undefined {
        const row = (kind === 'index' ? this.choices?.indexes : this.choices?.projects)?.find(item => item.id === id);
        if (!row) { return undefined; }
        return { ...(typeof row.name === 'string' && row.name ? { name: row.name } : {}), ...(typeof row.description === 'string' && row.description ? { description: row.description } : {}) };
    }
    /** (U105) Read the choices when none are loaded (names for the Agents screen); a failure leaves ids. */
    async loadChoices(): Promise<void> {
        if (this.choices || !this.available()) { return; }
        try { await this.readSearchScope(); } catch (error) { this.log(`AIRK: search choices for agent names could not be read: ${error instanceof Error ? error.message : String(error)}`); }
    }
    private require(): void {
        if (!this.available()) { throw new WindowScopeError('window_scope_unavailable', 'The window search scope is not available from this AIRK Service (window-scope/1).'); }
    }

    /** True while a granted scope is held: unselected reads run under it. */
    get active(): boolean { return this.current !== undefined && this.available(); }
    /** Identity of the granted scope (changes reset evidence); undefined without one. */
    identity(): string | undefined { return this.active ? JSON.stringify([this.current!.scopeIdentity, this.current!.selection]) : undefined; }
    /** The user's checked choice (not the members or sets the grant added). */
    searchScopeSelection(): { index_ids: string[]; project_ids: string[]; active: boolean } {
        const current = this.current;
        return { index_ids: [...current?.chosenIndexIds ?? []], project_ids: [...current?.chosenProjectIds ?? []], active: this.active };
    }
    /** How this window's index is read when no scope was chosen (U103); undefined until resolved. */
    readRoute(): ReadRouteView | undefined { return this.available() ? this.route : undefined; }
    searchScopeChoices(): SearchScopeChoices | undefined {
        if (!this.choices) { return undefined; }
        const selection = this.searchScopeSelection();
        return { ...this.choices, selectedIndexIds: selection.index_ids, selectedProjectIds: selection.project_ids, active: selection.active, outOfDate: this.stale,
            ...(selection.active && this.current?.auto ? { automatic: true } : {}) };
    }
    searchScopeSummary(): SearchScopeSummary {
        const available = this.available(), current = available ? this.current : undefined;
        const route = this.readRoute();
        return {
            available, active: !!current, outOfDate: available && this.stale, remembered: !!remembered(this.store.get(WINDOW_SCOPE_KEY)),
            ...(current?.auto ? { automatic: true } : {}),
            ...(current ? { label: current.label, indexIds: current.selection.index_ids, projectIds: current.selection.project_ids,
                scopeIdentity: current.scopeIdentity, catalogRevision: current.catalogRevision } : {}),
            ...(route ? { route: { ...route } } : {}),
        };
    }
    /** The Workbench entry for the granted scope; reads under it name no selection. */
    spec(): LaunchSpec | undefined {
        const current = this.active ? this.current : undefined, base = this.base();
        if (!current || !base) { return undefined; }
        const key = windowScopeKey(current.selection);
        return { command: base.command, root: base.root, dataDir: '', collection: 'window-scope', name: `Search scope: ${current.label}`,
            service: { installDir: base.installDir, project: key }, windowScope: { key, label: current.label } };
    }

    /** Read the choices: both kinds' first pages at one catalog revision, or the next page of one kind. */
    async readSearchScope(kind?: 'indexes' | 'projects', next = false): Promise<SearchScopeChoices> {
        this.require();
        const sequence = ++this.reads;
        const prior = this.choices;
        if (next && (!prior || !kind)) { throw new WindowScopeError('catalog_continuation_unavailable', 'Load the search choices first.'); }
        let choices: SearchScopeChoices = next ? { ...prior! } : { indexes: [], projects: [], selectedIndexIds: [], selectedProjectIds: [], active: false, responses: [] };
        for (const category of next ? [kind!] : ['indexes', 'projects'] as const) {
            const continuation = category === 'indexes' ? choices.indexContinuation : choices.projectContinuation;
            if (next && !continuation) { throw new WindowScopeError('catalog_continuation_unavailable', 'No more choices to load.'); }
            const expected = choices.revision ?? null;
            const response = await this.page({ kind: category, index_id: null, project_id: null, after: next ? continuation : null, expected_catalog_revision: expected });
            if (sequence !== this.reads) { throw new WindowScopeError('request_superseded', 'A newer read of the search choices replaced this one.'); }
            const body = catalogObject(response.page), namespace = catalogHex(body.namespace), revision = catalogRevision(body.revision);
            if (choices.namespace !== undefined && (namespace !== choices.namespace || revision !== choices.revision)) {
                throw new WindowScopeError('catalog_revision_changed_refresh', 'The index library changed while loading. Refresh the choices.');
            }
            const rows = catalogItems(response), expectedKind = category === 'indexes' ? 'index' : 'project';
            if (rows.some(row => row.kind !== expectedKind)) { throw new WindowScopeError('crossed_search_scope_page', 'The service answered another kind of catalog page.'); }
            const cursor = body.continuation === null || body.continuation === undefined ? null : catalogObject(body.continuation);
            if (cursor && (cursor.namespace !== namespace || cursor.revision !== revision || cursor.kind !== category || cursor.index_id !== null || cursor.project_id !== null
                || next && JSON.stringify(cursor) === JSON.stringify(continuation))) {
                throw new WindowScopeError('crossed_search_scope_continuation', 'The service answered a continuation of another page.');
            }
            const values = [...(next ? category === 'indexes' ? choices.indexes : choices.projects : []), ...rows.map(row => row.value)];
            if (new Set(values.map(value => value.id)).size !== values.length) { throw new WindowScopeError('search_scope_duplicate_items', 'The service listed a choice twice.'); }
            if (values.length > DISPLAY_LIMIT) { throw new WindowScopeError('search_scope_display_limit', `At most ${DISPLAY_LIMIT} choices are shown.`); }
            choices = category === 'indexes'
                ? { ...choices, namespace, revision, indexes: values, indexContinuation: cursor, indexResponses: [...(next ? choices.indexResponses ?? [] : []), response] }
                : { ...choices, namespace, revision, projects: values, projectContinuation: cursor, projectResponses: [...(next ? choices.projectResponses ?? [] : []), response] };
            choices.responses = [...choices.indexResponses ?? [], ...choices.projectResponses ?? [], ...choices.membershipResponses ?? []];
        }
        this.choices = choices; this.changed();
        return this.searchScopeChoices()!;
    }

    /** Grant `selection` at the loaded revision as a union (U104): every member of the checked index sets
     * plus every checked index, each read through the index sets that hold it; an index in no index set
     * first gets a visible index set named after it. Nothing selected releases. */
    async applySearchScope(selection: Record<string, unknown>): Promise<Record<string, unknown>> {
        this.require();
        const project_ids = ids(selection.project_ids, 'prj_'), index_ids = ids(selection.index_ids, 'idx_');
        if (!project_ids.length && !index_ids.length) { await this.clearSearchScope(); return { status: 'scope_released' }; }
        await this.settled();
        const choices = this.choices;
        if (choices?.namespace === undefined || choices.revision === undefined) { throw new WindowScopeError('search_scope_choices_required', 'Load the search choices first.'); }
        const result = await this.grantChoice(choices.namespace, choices.revision, project_ids, index_ids, 'union', true, true);
        await this.store.update(WINDOW_SCOPE_CLEARED_KEY, undefined);
        return result;
    }

    /** (U104) Save the checked indexes and the members of the checked index sets as a new named index set. */
    async saveIndexSet(nameInput: unknown, selection: Record<string, unknown>): Promise<Record<string, unknown>> {
        this.require();
        const name = typeof nameInput === 'string' ? nameInput.trim() : '';
        if (!name || name.length > 128 || /[\u0000-\u001f\u007f]/.test(name)) { throw new WindowScopeError('index_set_name_required', 'Enter a name for the new index set (at most 128 characters).'); }
        if (!this.library) { throw new WindowScopeError('index_set_create_unavailable', 'Creating index sets is not available in this window.'); }
        const project_ids = ids(selection.project_ids, 'prj_'), index_ids = ids(selection.index_ids, 'idx_');
        await this.settled();
        const choices = this.choices;
        if (choices?.namespace === undefined || choices.revision === undefined) { throw new WindowScopeError('search_scope_choices_required', 'Load the search choices first.'); }
        const members = project_ids.length ? await this.members(project_ids, choices.revision, choices.namespace) : [];
        const all = [...index_ids, ...members.filter(id => !index_ids.includes(id))];
        if (!all.length) { throw new WindowScopeError('window_scope_empty', 'Check at least one index, or an index set that holds indexes, to save as an index set.'); }
        const outcome = await this.library.createSet(name, all);
        await this.readSearchScope(); await this.readIndexSets();
        return outcome;
    }

    /** (U105) Set (`text`) or clear (undefined) the description of an index or index set, then read the choices again. */
    async describe(kindInput: unknown, idInput: unknown, textInput: unknown): Promise<Record<string, unknown>> {
        this.require();
        if (kindInput !== 'index' && kindInput !== 'set') { throw new WindowScopeError('description_target_invalid', 'Choose an index or an index set to describe.'); }
        const kind: DescriptionKind = kindInput;
        let id: string;
        try { id = catalogId(idInput, kind === 'index' ? 'idx_' : 'prj_'); } catch { throw new WindowScopeError('description_target_invalid', 'Choose an index or an index set to describe.'); }
        const text = textInput === undefined ? undefined : typeof textInput === 'string' ? textInput : null;
        if (text === null || text !== undefined && (!text.trim() || new TextEncoder().encode(text).length > DESCRIPTION_MAX_BYTES || /[\u0000-\u0009\u000b-\u001f\u007f]/.test(text))) {
            throw new WindowScopeError('description_invalid', `A description is 1 to ${DESCRIPTION_MAX_BYTES} bytes of text; line breaks are allowed, other control characters are not.`);
        }
        if (!this.library?.describe) { throw new WindowScopeError('description_unavailable', 'Editing descriptions is not available in this window.'); }
        const outcome = await this.library.describe(kind, id, text);
        await this.readSearchScope(); await this.readIndexSets();
        return outcome;
    }

    /** Re-read the catalog, then grant these ids at its current revision (Recommendations and agent search drafts).
     * The named indexes narrow the named sets, exactly as the native selection says. */
    async selectIds(project_ids: readonly string[] | undefined, index_ids: readonly string[] | undefined): Promise<Record<string, unknown>> {
        this.require();
        const projects = ids(project_ids, 'prj_'), indexes = ids(index_ids, 'idx_');
        if (!projects.length && !indexes.length) { throw new WindowScopeError('window_scope_project_required', 'The suggested scope names no index or index set; choose one in Indexing › Search scope.'); }
        await this.settled();
        await this.readSearchScope();
        const result = await this.grantChoice(this.choices!.namespace!, this.choices!.revision!, projects, indexes, 'exact', false, true);
        await this.store.update(WINDOW_SCOPE_CLEARED_KEY, undefined);
        return result;
    }

    /** Every membership at the loaded revision (bounded pages): which index sets hold each index.
     * A failure is kept as its code; the choices stay usable and the service validates Apply. */
    async readIndexSets(): Promise<void> {
        const choices = this.choices;
        if (!this.available() || choices?.namespace === undefined || choices.revision === undefined) { return; }
        const { namespace, revision } = choices, sets: Record<string, string[]> = {}, responses: CatalogObject[] = [];
        let after: CatalogObject | null = null, complete = false, error: string | undefined;
        try {
            for (let page = 0; page < INDEX_PAGES; page++) {
                const response = await this.page({ kind: 'memberships', index_id: null, project_id: null, after, expected_catalog_revision: revision });
                const body = catalogObject(response.page);
                if (catalogHex(body.namespace) !== namespace || catalogRevision(body.revision) !== revision) {
                    throw new WindowScopeError('catalog_revision_changed_refresh', 'The index library changed while loading. Refresh the choices.');
                }
                responses.push(response);
                for (const row of catalogItems(response)) {
                    if (row.kind !== 'membership') { throw new WindowScopeError('crossed_index_set_membership', 'The service answered another kind of catalog page.'); }
                    const index = catalogId(row.value.index_id, 'idx_'), set = catalogId(row.value.project_id, 'prj_');
                    if (!Array.isArray(row.value.scope) || !row.value.scope.length) { continue; }
                    const held = sets[index] ??= [];
                    if (!held.includes(set)) { held.push(set); }
                }
                if (body.continuation === null || body.continuation === undefined) { complete = true; break; }
                after = catalogObject(body.continuation);
            }
        } catch (failure) {
            error = failure instanceof WindowScopeError ? failure.code : 'index_set_read_failed';
            this.log(`AIRK: index set memberships could not be read (${error}).`);
        }
        if (this.choices !== choices) { return; }
        this.choices = { ...choices, indexSets: sets, indexSetsComplete: complete && !error, membershipResponses: responses,
            ...(error ? { indexSetsError: error } : {}), responses: [...choices.indexResponses ?? [], ...choices.projectResponses ?? [], ...responses] };
        this.changed();
    }

    /** Release the scope. `user` (the default): the user cleared it, so no automatic scope replaces it. */
    async clearSearchScope(user = true): Promise<void> {
        // A user's change waits for an automatic resolution in flight, so that grant cannot land after it.
        if (user) { await this.settled(); }
        if (this.available()) {
            const outcome = await this.session.scopeRelease();
            if (outcome.status === 'refused') { throw new WindowScopeError(outcome.code, refusalMessage(outcome.code)); }
            if (outcome.status !== 'scope_released') { throw new WindowScopeError('session_invalid_response', 'The AIRK session answered the release unexpectedly.'); }
        }
        this.current = undefined; this.stale = false;
        if (user) {
            await this.store.update(WINDOW_SCOPE_KEY, undefined);
            await this.store.update(WINDOW_SCOPE_CLEARED_KEY, true);
            if (this.route) { this.route = { state: 'cleared' }; }
        }
        this.changed();
        if (user) { this.agentScopeChanged({ scope: null, user: true }); }
    }

    /**
     * Grant a checked choice. `union` (U104): the members of `project_ids` plus every index of `index_ids`
     * that is not one of them, the latter through the index sets that hold it (added to the grant); with
     * no extra index the grant names the sets' members. `exact`: the indexes narrow the sets (Recommendations,
     * agent drafts and earlier remembered choices). Without sets both read the indexes through their sets.
     * `create`: an index in no index set first gets a visible index set named after it, then the catalog is
     * read again and the choice granted at its new revision.
     */
    private async grantChoice(namespace: string, revision: number, project_ids: string[], index_ids: string[], mode: Mode, create = false, user = false): Promise<Record<string, unknown>> {
        if (mode === 'exact' && project_ids.length) {
            const members = await this.members(project_ids, revision, namespace);
            const granted = index_ids.length ? index_ids : members;
            const outside = granted.filter(id => !members.includes(id));
            if (outside.length) { throw new WindowScopeError('window_scope_index_outside_sets', `Choose indexes that belong to the chosen index sets (${outside.join(', ')} does not).`); }
            return this.grant(namespace, revision, project_ids, granted, { chosenProjectIds: project_ids, chosenIndexIds: index_ids,
                label: index_ids.length ? `${this.names('projects', project_ids)} (${this.names('indexes', index_ids)})` : this.names('projects', project_ids),
                remember: { project_ids, index_ids: granted, explicit: index_ids.length > 0 },
                // Open reads take a set whole (U106a): indexes narrowing the sets go to the agent as indexes.
                agent: index_ids.length ? { index_ids: granted, project_ids: [] } : { index_ids: [], project_ids }, user });
        }
        const members = project_ids.length ? await this.members(project_ids, revision, namespace) : [];
        const extra = index_ids.filter(id => !members.includes(id));
        const added: string[] = [];
        if (extra.length) {
            const map = await this.setsOf(extra, revision, namespace);
            const missing = extra.filter(id => !map.get(id)?.length);
            if (missing.length && create && this.library) {
                for (const id of missing) {
                    const name = this.names('indexes', [id]);
                    this.log(`AIRK: creating the index set "${name}" for ${id} (U104).`);
                    await this.library.createSet(name, [id]);
                }
                await this.readSearchScope(); await this.readIndexSets();
                const fresh = this.choices!;
                return this.grantChoice(fresh.namespace!, fresh.revision!, project_ids, index_ids, mode, false, user);
            }
            if (missing.length) {
                const one = missing.length === 1;
                throw new WindowScopeError('window_scope_index_not_readable',
                    `${missing.map(id => this.names('indexes', [id])).join(', ')} ${one ? 'is' : 'are'} in no index set, so ${one ? 'it is' : 'they are'} not readable yet. Apply ${one ? 'it' : 'them'} in Indexing › Search scope to create an index set named after ${one ? 'it' : 'each'}, or make ${one ? 'it' : 'them'} readable in AIRK Admin (Index library).`,
                    { status: 'refused', code: 'window_scope_index_not_readable', index_ids: missing });
            }
            for (const id of extra) { for (const set of map.get(id)!) { if (!project_ids.includes(set) && !added.includes(set)) { added.push(set); } } }
        }
        const sets = [...project_ids, ...added], granted = extra.length ? [...members, ...extra] : members;
        if (sets.length > SCOPE_MAX_SELECTION) { throw new WindowScopeError('window_scope_selection_invalid', `A search scope holds at most ${SCOPE_MAX_SELECTION} index sets; this selection needs ${sets.length}.`); }
        const label = !project_ids.length ? `${this.names('projects', sets)} (${this.names('indexes', index_ids)})`
            : extra.length ? `${this.names('projects', project_ids)} + ${this.names('indexes', extra)}` : this.names('projects', project_ids);
        const remember: Omit<Remembered, 'namespace'> = mode === 'union'
            ? { project_ids, index_ids, explicit: index_ids.length > 0, union: true }
            : { project_ids: sets, index_ids, explicit: true, resolved: true };
        // Union: the agent reads the checked sets whole plus the checked indexes; an exact pick of indexes alone names them.
        const agent = mode === 'union' ? { index_ids, project_ids } : { index_ids: granted, project_ids: [] };
        return this.grant(namespace, revision, sets, granted, { chosenProjectIds: project_ids, chosenIndexIds: index_ids, label, remember, agent, user });
    }
    /** Display names (else IDs) of loaded choices, bounded for a label. */
    private names(kind: 'indexes' | 'projects', list: readonly string[]): string {
        const rows = kind === 'indexes' ? this.choices?.indexes : this.choices?.projects;
        return list.map(id => { const row = rows?.find(item => item.id === id); return typeof row?.name === 'string' && row.name ? row.name : id; }).join(', ').slice(0, 512);
    }

    /**
     * (U103) Read this window's index without a chosen scope: the catalog index whose backing
     * binding is `binding`, through the index sets that hold it. A scope the user applied or
     * cleared is kept; an index in no index set is `not_readable` (never read by binding); a
     * project the catalog does not list keeps the binding route. Called again after a refresh:
     * the automatic scope is granted again only when the catalog revision or the sets changed.
     */
    ensureDefault(binding: string | undefined): Promise<void> {
        if (!this.available() || !binding || !/^[0-9a-f]{64}$/.test(binding)) { return Promise.resolve(); }
        if (this.resolving?.binding === binding) { return this.resolving.work; }
        // Resolutions run one at a time; a newer binding supersedes an older one before it grants.
        const ticket = ++this.defaultTicket, previous = this.resolving?.work;
        if (this.routeBinding !== binding) { this.routeBinding = binding; if (this.route) { this.setRoute({ state: 'checking' }); } }
        const work: Promise<void> = (async () => {
            await previous?.catch(() => undefined);
            if (ticket === this.defaultTicket) { await this.resolveDefault(binding, ticket); }
        })().finally(() => { if (this.resolving?.work === work) { this.resolving = undefined; } });
        this.resolving = { binding, work };
        return work;
    }
    /** Wait for an automatic resolution in flight (its failures are its own). */
    private async settled(): Promise<void> {
        const work = this.resolving?.work;
        if (work) { await work.catch(() => undefined); }
    }
    private setRoute(route: ReadRouteView): void {
        if (JSON.stringify(route) === JSON.stringify(this.route)) { return; }
        this.route = route; this.changed();
    }
    private async resolveDefault(binding: string, ticket: number): Promise<void> {
        if (this.current && !this.current.auto) { this.setRoute({ state: 'scope', automatic: false }); return; }
        if (this.store.get(WINDOW_SCOPE_CLEARED_KEY) === true) { this.setRoute({ state: 'cleared' }); return; }
        if (!this.current && remembered(this.store.get(WINDOW_SCOPE_KEY))) {
            await this.restore();
            if (this.current) { this.setRoute({ state: 'scope', automatic: false }); return; }
            // The remembered choice could not be applied: it stays out of date for the user to reselect.
            return;
        }
        if (!this.route || this.route.state === 'failed') { this.setRoute({ state: 'checking' }); }
        try {
            const found = await this.findIndex(binding);
            if (!found.index) {
                if (this.current?.auto) { await this.clearSearchScope(false); }
                this.setRoute({ state: 'binding' });
                return;
            }
            const indexId = catalogId(found.index.id, 'idx_'), indexName = typeof found.index.name === 'string' && found.index.name ? found.index.name : indexId;
            const sets = (await this.setsOf([indexId], found.revision, found.namespace)).get(indexId) ?? [];
            if (!sets.length) {
                if (this.current?.auto) { await this.clearSearchScope(false); }
                this.setRoute({ state: 'not_readable', indexId, indexName, code: 'index_read_unavailable',
                    message: `${indexName} is in no index set, so it is not readable yet. Check it in Indexing › Search scope and apply to create an index set named after it, or make it readable in AIRK Admin (Index library).` });
                return;
            }
            const current = this.current;
            // Same revision and sets: nothing to grant again (a refused read at this revision stays out of date).
            if (current?.auto?.binding === binding && current.catalogRevision === found.revision
                && sameSet(current.selection.index_ids, [indexId]) && sameSet(current.selection.project_ids, sets)) {
                this.setRoute({ state: 'scope', automatic: true, indexId, indexName });
                return;
            }
            // A user choice made while the catalog was read wins.
            if (this.current && !this.current.auto) { this.setRoute({ state: 'scope', automatic: false }); return; }
            if (this.store.get(WINDOW_SCOPE_CLEARED_KEY) === true) { this.setRoute({ state: 'cleared' }); return; }
            if (ticket !== this.defaultTicket) { return; }
            // The index is a member of every set that holds it (the same memberships, read by set).
            const members = await this.members(sets, found.revision, found.namespace);
            if (!members.includes(indexId)) { throw new WindowScopeError('window_scope_index_outside_sets', `Choose indexes that belong to the chosen index sets (${indexId} does not).`); }
            await this.grant(found.namespace, found.revision, sets, [indexId], { chosenProjectIds: [], chosenIndexIds: [indexId], label: indexName, auto: { binding },
                agent: { index_ids: [indexId], project_ids: [] }, user: false });
            this.setRoute({ state: 'scope', automatic: true, indexId, indexName });
        } catch (error) {
            const code = error instanceof WindowScopeError ? error.code : typeof (error as { code?: unknown })?.code === 'string' ? String((error as { code: string }).code) : 'catalog_read_failed';
            const message = error instanceof Error ? error.message : String(error);
            this.log(`AIRK: the window's index could not be resolved (${code}): ${message}`);
            this.setRoute({ state: 'failed', code, message: message.slice(0, 600) });
        }
    }
    /** The live catalog index backed by `binding`, read page by page at one revision. */
    private async findIndex(binding: string): Promise<{ namespace: string; revision: number; index?: CatalogObject }> {
        let after: CatalogObject | null = null, namespace: string | undefined, revision: number | undefined;
        for (let page = 0; page < INDEX_PAGES; page++) {
            const response = await this.page({ kind: 'indexes', index_id: null, project_id: null, after, expected_catalog_revision: revision ?? null });
            const body = catalogObject(response.page), pageNamespace = catalogHex(body.namespace), pageRevision = catalogRevision(body.revision);
            if (namespace !== undefined && (pageNamespace !== namespace || pageRevision !== revision)) {
                throw new WindowScopeError('catalog_revision_changed_refresh', 'The index library changed while loading. Refresh the choices.');
            }
            namespace = pageNamespace; revision = pageRevision;
            for (const row of catalogItems(response)) {
                if (row.kind !== 'index') { throw new WindowScopeError('crossed_search_scope_page', 'The service answered another kind of catalog page.'); }
                if (row.value.tombstone !== true && catalogObject(row.value.backing ?? {}).binding === binding) { return { namespace, revision, index: row.value }; }
            }
            if (body.continuation === null || body.continuation === undefined) { return { namespace, revision }; }
            after = catalogObject(body.continuation);
        }
        throw new WindowScopeError('search_scope_display_limit', `The index library has more than ${INDEX_PAGES * SCOPE_PAGE_LIMIT} indexes; choose a search scope in Indexing › Search scope.`);
    }
    /** For each index, the index sets whose membership grants read authority (a non-empty scope). */
    private async setsOf(index_ids: readonly string[], revision: number, namespace: string): Promise<Map<string, string[]>> {
        const found = new Map<string, string[]>();
        for (const index of index_ids) {
            const sets: string[] = [], seen = new Set<string>();
            let after: CatalogObject | null = null;
            for (let page = 0; ; page++) {
                if (page >= MEMBERSHIP_PAGES) { throw new WindowScopeError('index_set_membership_limit', 'An index belongs to more index sets than a search scope can hold.'); }
                const response = await this.page({ kind: 'memberships', index_id: index, project_id: null, after, expected_catalog_revision: revision });
                const body = catalogObject(response.page);
                if (catalogHex(body.namespace) !== namespace || catalogRevision(body.revision) !== revision) {
                    throw new WindowScopeError('catalog_revision_changed_refresh', 'The index library changed while loading. Refresh the choices.');
                }
                for (const row of catalogItems(response)) {
                    if (row.kind !== 'membership' || row.value.index_id !== index) { throw new WindowScopeError('crossed_index_set_membership', 'The service answered memberships of another index.'); }
                    const set = catalogId(row.value.project_id, 'prj_');
                    if (Array.isArray(row.value.scope) && row.value.scope.length && !sets.includes(set)) { sets.push(set); }
                }
                if (body.continuation === null || body.continuation === undefined) { break; }
                after = catalogObject(body.continuation);
                const token = JSON.stringify(after);
                if (seen.has(token)) { throw new WindowScopeError('index_set_continuation_repeated', 'The service repeated a memberships page.'); }
                seen.add(token);
            }
            found.set(index, sets);
        }
        return found;
    }
    /** A window-scoped read was refused (contracts §3.7 Q4 already re-selected once). */
    refused(code: string): void {
        if (!WINDOW_SCOPE_REFUSALS.includes(code) || !this.current || this.stale) { return; }
        this.stale = true; this.log(`AIRK: the window search scope is out of date (${code}).`); this.changed();
        // An automatic scope follows the catalog by itself: read it again and grant at its current revision.
        const auto = this.current.auto;
        if (auto) { void this.ensureDefault(auto.binding); }
    }
    /** The grant is gone (session ended or service restarted); the remembered choice stays for `restore`. */
    lost(): void {
        if (!this.current && !this.stale) { return; }
        this.current = undefined; this.stale = false; this.changed();
    }

    /** Apply the remembered choice again after reading the catalog at its current revision. */
    restore(): Promise<void> {
        if (this.current || !this.available()) { return Promise.resolve(); }
        const saved = remembered(this.store.get(WINDOW_SCOPE_KEY));
        if (!saved) { return Promise.resolve(); }
        this.restoring ??= (async () => {
            try {
                await this.readSearchScope();
                const choices = this.choices!;
                if (choices.namespace !== saved.namespace) { this.stale = true; this.changed(); this.log('AIRK: the remembered search scope belongs to another index library; reselect it.'); return; }
                // A remembered choice never creates an index set; an index that lost its sets is reselected.
                if (saved.union) { await this.grantChoice(choices.namespace, choices.revision!, saved.project_ids, saved.index_ids, 'union'); }
                else if (saved.resolved) { await this.grantChoice(choices.namespace, choices.revision!, [], saved.index_ids, 'exact'); }
                else { await this.grantChoice(choices.namespace, choices.revision!, saved.project_ids, saved.explicit ? saved.index_ids : [], 'exact'); }
            } catch (error) {
                this.log(`AIRK: the remembered search scope was not applied: ${error instanceof Error ? error.message : String(error)}`);
                if (!this.stale) { this.stale = true; this.changed(); }
            }
        })().finally(() => { this.restoring = undefined; });
        return this.restoring;
    }

    /** Send one `scope_select` for exactly `project_ids`/`index_ids`; only a user's choice is remembered. */
    private async grant(namespace: string, revision: number, project_ids: string[], index_ids: string[], options: GrantOptions): Promise<Record<string, unknown>> {
        if (!index_ids.length) { throw new WindowScopeError('window_scope_empty', 'The chosen index sets contain no index. Add indexes to them in AIRK Admin.'); }
        if (index_ids.length > SCOPE_MAX_SELECTION) { throw new WindowScopeError('window_scope_selection_invalid', `A search scope holds at most ${SCOPE_MAX_SELECTION} indexes; this selection has ${index_ids.length}.`); }
        const selection: WindowSelection = { namespace, expected_catalog_revision: revision, index_ids, project_ids };
        const outcome = await this.session.scopeSelect(selection);
        if (outcome.status === 'refused') {
            if (outcome.code === 'scoped_read_unavailable' && this.current) { this.stale = true; this.changed(); }
            throw new WindowScopeError(outcome.code, refusalMessage(outcome.code), { ...outcome });
        }
        if (outcome.status !== 'scope') { throw new WindowScopeError('session_invalid_response', 'The AIRK session answered the scope request unexpectedly.'); }
        this.current = { selection, scopeIdentity: outcome.scope_identity, catalogRevision: outcome.catalog_revision, expiresAtMs: outcome.expires_at_ms,
            label: options.label, explicit: options.chosenIndexIds.length > 0,
            chosenIndexIds: [...options.chosenIndexIds], chosenProjectIds: [...options.chosenProjectIds], ...(options.auto ? { auto: options.auto } : {}),
            agent: { index_ids: [...options.agent.index_ids], project_ids: [...options.agent.project_ids] } };
        this.stale = false;
        // An automatic scope is derived again from the catalog; only the user's choice is remembered.
        if (options.remember && !options.auto) {
            await this.store.update(WINDOW_SCOPE_KEY, { namespace, ...options.remember } satisfies Remembered);
            if (this.route) { this.route = { state: 'scope', automatic: false }; }
        }
        this.changed();
        this.agentScopeChanged({ scope: { index_ids: [...options.agent.index_ids], project_ids: [...options.agent.project_ids] }, user: options.user });
        return { ...outcome };
    }
    /** Member indexes of the chosen index sets at `revision` (memberships pages, bounded). A membership
     * with an empty scope grants no read authority (selection-authority spec §3.5) and is left out. */
    private async members(project_ids: readonly string[], revision: number, namespace: string): Promise<string[]> {
        const found: string[] = [];
        for (const project of project_ids) {
            let after: CatalogObject | null = null;
            const seen = new Set<string>();
            for (let page = 0; ; page++) {
                if (page >= MEMBERSHIP_PAGES) { throw new WindowScopeError('index_set_membership_limit', 'An index set has more members than a search scope can hold.'); }
                const response = await this.page({ kind: 'memberships', index_id: null, project_id: project, after, expected_catalog_revision: revision });
                const body = catalogObject(response.page);
                if (catalogHex(body.namespace) !== namespace || catalogRevision(body.revision) !== revision) {
                    throw new WindowScopeError('catalog_revision_changed_refresh', 'The index library changed while loading. Refresh the choices.');
                }
                for (const row of catalogItems(response)) {
                    if (row.kind !== 'membership' || row.value.project_id !== project) { throw new WindowScopeError('crossed_index_set_membership', 'The service answered memberships of another index set.'); }
                    const id = catalogId(row.value.index_id, 'idx_');
                    if (Array.isArray(row.value.scope) && row.value.scope.length && !found.includes(id)) { found.push(id); }
                }
                if (body.continuation === null || body.continuation === undefined) { break; }
                after = catalogObject(body.continuation);
                const token = JSON.stringify(after);
                if (seen.has(token)) { throw new WindowScopeError('index_set_continuation_repeated', 'The service repeated a memberships page.'); }
                seen.add(token);
            }
        }
        return found;
    }
    private async page(request: Record<string, unknown>): Promise<Record<string, unknown>> {
        const response = await this.session.control({ catalog_page: { request: { ...request, limit: SCOPE_PAGE_LIMIT, max_bytes: SCOPE_PAGE_BYTES } } });
        if (response?.status !== 'catalog_page') {
            const code = typeof response?.code === 'string' ? response.code : `unexpected_${typeof response?.status === 'string' ? response.status : 'response'}`;
            throw new WindowScopeError(code, `The index library could not be read (${code}).`, response);
        }
        return response;
    }
}
