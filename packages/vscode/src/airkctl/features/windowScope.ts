// SPDX-License-Identifier: Apache-2.0
//! The window search scope (U77, plan §8.1 "Window search scope is chosen
//! among existing indexes through the session"; contracts §2.5 W4, §3.7).
//! Choices are read read-only through the session's metadata relay
//! (`catalog_page`, kinds `indexes` and `projects`) at one catalog revision;
//! a choice is granted with `scope_select` and dropped with `scope_release`.
//! Nothing here creates or changes an index, an index set or an agent grant:
//! index sets are created in AIRK Admin. The last choice is remembered per
//! workspace and applied again after a reconnect only after the catalog is
//! read again at its current revision; a stale revision is never applied
//! blindly. A read refused `scoped_read_unavailable` (after the session's
//! one Q4 re-select) marks the scope out of date for the user to reselect.
import { createHash } from 'node:crypto';
import type { SessionOutcome, WindowSelection } from '../../generated/i2';
import { catalogHex, catalogId, catalogItems, catalogObject, catalogRevision, type CatalogObject } from '../../indexingCatalog';
import type { LaunchSpec } from '../../model';
import type { SearchScopeSummary } from '../../explorerContracts';

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
export interface ScopeStore { get<T>(key: string): T | undefined; update(key: string, value: unknown): PromiseLike<void> }
/** What a window-scope launch spec is built from (the verified installation). */
export interface ScopeBase { readonly command: string; readonly installDir: string; readonly root: string }

/** The choices the Indexing page renders (`CatalogView.searchScope`). */
export interface SearchScopeChoices {
    indexes: CatalogObject[]; projects: CatalogObject[]; selectedIndexIds: string[]; selectedProjectIds: string[];
    managedProjectIds: string[]; active: boolean; outOfDate?: boolean; namespace?: string; revision?: number;
    indexContinuation?: CatalogObject | null; projectContinuation?: CatalogObject | null;
    indexResponses?: CatalogObject[]; projectResponses?: CatalogObject[]; responses: CatalogObject[];
}
interface Active {
    readonly selection: WindowSelection; readonly scopeIdentity: string; readonly catalogRevision: number;
    readonly expiresAtMs: number; readonly label: string; readonly explicit: boolean;
}
interface Remembered { namespace: string; project_ids: string[]; index_ids: string[]; explicit: boolean }

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
        return { namespace: catalogHex(item.namespace), project_ids: ids(item.project_ids, 'prj_'), index_ids: ids(item.index_ids, 'idx_'), explicit: item.explicit };
    } catch { return undefined; }
}
/** A stable 64-hex key of one selection (Workbench history), independent of the catalog revision. */
export function windowScopeKey(selection: Pick<WindowSelection, 'namespace' | 'index_ids' | 'project_ids'>): string {
    return createHash('sha256').update(`airk/window-scope/1\u0000${JSON.stringify([selection.namespace, [...selection.project_ids].sort(), [...selection.index_ids].sort()])}`).digest('hex');
}

export class WindowScope {
    private choices?: SearchScopeChoices;
    private current?: Active;
    private stale = false;
    private reads = 0;
    private restoring?: Promise<void>;
    private readonly listeners = new Set<() => void>();
    /** Changes on every published change (the Explorer's delivery tag). */
    generation = 0;

    constructor(private readonly session: ScopeSession, private readonly capabilities: () => readonly string[] | undefined,
        private readonly store: ScopeStore, private readonly base: () => ScopeBase | undefined, private readonly log: (line: string) => void = () => {}) {}

    available(): boolean { return this.capabilities()?.includes(WINDOW_SCOPE_CAPABILITY) === true; }
    onDidChange(listener: () => void): { dispose(): void } { this.listeners.add(listener); return { dispose: () => { this.listeners.delete(listener); } }; }
    private changed(): void { this.generation++; for (const listener of [...this.listeners]) { try { listener(); } catch { /* Listener failures stay local. */ } } }
    private require(): void {
        if (!this.available()) { throw new WindowScopeError('window_scope_unavailable', 'The window search scope is not available from this AIRK Service (window-scope/1).'); }
    }

    /** True while a granted scope is held: unselected reads run under it. */
    get active(): boolean { return this.current !== undefined && this.available(); }
    /** Identity of the granted scope (changes reset evidence); undefined without one. */
    identity(): string | undefined { return this.active ? JSON.stringify([this.current!.scopeIdentity, this.current!.selection]) : undefined; }
    searchScopeSelection(): { index_ids: string[]; project_ids: string[]; managedProjectIds: string[]; active: boolean } {
        const selection = this.current?.selection;
        return { index_ids: [...selection?.index_ids ?? []], project_ids: [...selection?.project_ids ?? []], managedProjectIds: [], active: this.active };
    }
    searchScopeChoices(): SearchScopeChoices | undefined {
        if (!this.choices) { return undefined; }
        const selection = this.searchScopeSelection();
        return { ...this.choices, selectedIndexIds: selection.index_ids, selectedProjectIds: selection.project_ids, active: selection.active, outOfDate: this.stale };
    }
    searchScopeSummary(): SearchScopeSummary {
        const available = this.available(), current = available ? this.current : undefined;
        return {
            available, active: !!current, outOfDate: available && this.stale, remembered: !!remembered(this.store.get(WINDOW_SCOPE_KEY)),
            ...(current ? { label: current.label, indexIds: current.selection.index_ids, projectIds: current.selection.project_ids,
                scopeIdentity: current.scopeIdentity, catalogRevision: current.catalogRevision } : {}),
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
        let choices: SearchScopeChoices = next ? { ...prior! } : { indexes: [], projects: [], selectedIndexIds: [], selectedProjectIds: [], managedProjectIds: [], active: false, responses: [] };
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
            choices.responses = [...choices.indexResponses ?? [], ...choices.projectResponses ?? []];
        }
        this.choices = choices; this.changed();
        return this.searchScopeChoices()!;
    }

    /** Grant `selection` (index sets, optionally narrowed to indexes) at the loaded revision; nothing selected releases. */
    async applySearchScope(selection: Record<string, unknown>): Promise<Record<string, unknown>> {
        this.require();
        const project_ids = ids(selection.project_ids, 'prj_'), index_ids = ids(selection.index_ids, 'idx_');
        if (!project_ids.length && !index_ids.length) { await this.clearSearchScope(); return { status: 'scope_released' }; }
        if (!project_ids.length) {
            throw new WindowScopeError('window_scope_project_required', 'Choose at least one index set. A search scope reads through index sets; create them in AIRK Admin.');
        }
        const choices = this.choices;
        if (choices?.namespace === undefined || choices.revision === undefined) { throw new WindowScopeError('search_scope_choices_required', 'Load the search choices first.'); }
        return this.grant(choices.namespace, choices.revision, project_ids, index_ids);
    }

    /** Re-read the catalog, then grant these ids at its current revision (Recommendations and agent search drafts). */
    async selectIds(project_ids: readonly string[] | undefined, index_ids: readonly string[] | undefined): Promise<Record<string, unknown>> {
        this.require();
        const projects = ids(project_ids, 'prj_'), indexes = ids(index_ids, 'idx_');
        if (!projects.length) { throw new WindowScopeError('window_scope_project_required', 'The suggested scope names no index set; choose one in Indexing › Search scope.'); }
        await this.readSearchScope();
        return this.grant(this.choices!.namespace!, this.choices!.revision!, projects, indexes);
    }

    async clearSearchScope(): Promise<void> {
        if (this.available()) {
            const outcome = await this.session.scopeRelease();
            if (outcome.status === 'refused') { throw new WindowScopeError(outcome.code, refusalMessage(outcome.code)); }
            if (outcome.status !== 'scope_released') { throw new WindowScopeError('session_invalid_response', 'The AIRK session answered the release unexpectedly.'); }
        }
        this.current = undefined; this.stale = false;
        await this.store.update(WINDOW_SCOPE_KEY, undefined);
        this.changed();
    }

    /** A window-scoped read was refused (contracts §3.7 Q4 already re-selected once). */
    refused(code: string): void {
        if (!WINDOW_SCOPE_REFUSALS.includes(code) || !this.current || this.stale) { return; }
        this.stale = true; this.log(`AIRK: the window search scope is out of date (${code}).`); this.changed();
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
                await this.grant(choices.namespace, choices.revision!, saved.project_ids, saved.explicit ? saved.index_ids : []);
            } catch (error) {
                this.log(`AIRK: the remembered search scope was not applied: ${error instanceof Error ? error.message : String(error)}`);
                if (!this.stale) { this.stale = true; this.changed(); }
            }
        })().finally(() => { this.restoring = undefined; });
        return this.restoring;
    }

    private async grant(namespace: string, revision: number, project_ids: string[], explicit: string[]): Promise<Record<string, unknown>> {
        const members = await this.members(project_ids, revision, namespace);
        const index_ids = explicit.length ? explicit : members;
        if (!index_ids.length) { throw new WindowScopeError('window_scope_empty', 'The chosen index sets contain no index. Add indexes to them in AIRK Admin.'); }
        const outside = index_ids.filter(id => !members.includes(id));
        if (outside.length) { throw new WindowScopeError('window_scope_index_outside_sets', `Choose indexes that belong to the chosen index sets (${outside.join(', ')} does not).`); }
        if (index_ids.length > SCOPE_MAX_SELECTION) { throw new WindowScopeError('window_scope_selection_invalid', `A search scope holds at most ${SCOPE_MAX_SELECTION} indexes.`); }
        const selection: WindowSelection = { namespace, expected_catalog_revision: revision, index_ids, project_ids };
        const outcome = await this.session.scopeSelect(selection);
        if (outcome.status === 'refused') {
            if (outcome.code === 'scoped_read_unavailable' && this.current) { this.stale = true; this.changed(); }
            throw new WindowScopeError(outcome.code, refusalMessage(outcome.code), { ...outcome });
        }
        if (outcome.status !== 'scope') { throw new WindowScopeError('session_invalid_response', 'The AIRK session answered the scope request unexpectedly.'); }
        this.current = { selection, scopeIdentity: outcome.scope_identity, catalogRevision: outcome.catalog_revision, expiresAtMs: outcome.expires_at_ms,
            label: this.label(selection, explicit.length > 0), explicit: explicit.length > 0 };
        this.stale = false;
        await this.store.update(WINDOW_SCOPE_KEY, { namespace, project_ids, index_ids, explicit: explicit.length > 0 } satisfies Remembered);
        this.changed();
        return { ...outcome };
    }
    /** Index set names, and the chosen indexes when the sets were narrowed (names, else IDs). */
    private label(selection: WindowSelection, narrowed: boolean): string {
        const name = (rows: CatalogObject[] | undefined, id: string) => { const row = rows?.find(item => item.id === id); return typeof row?.name === 'string' && row.name ? row.name : id; };
        const sets = selection.project_ids.map(id => name(this.choices?.projects, id)).join(', ');
        return (narrowed ? `${sets} (${selection.index_ids.map(id => name(this.choices?.indexes, id)).join(', ')})` : sets).slice(0, 512);
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
