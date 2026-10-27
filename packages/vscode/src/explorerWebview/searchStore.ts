// SPDX-License-Identifier: Apache-2.0
// One selected project's search, retained by the host for this window session.
import { useEffect, useState } from 'preact/hooks';
import type { SearchForm, SearchSessionView } from '../explorerContracts';
import { notify } from './api';

export const DEFAULT_SEARCH: SearchForm = { query: '', mode: 'policy', pathPrefix: '', limit: 20, maxBytes: 16384, fullChunks: false };
let current: SearchSessionView = { project: '', selectionEpoch: 0, form: DEFAULT_SEARCH, busy: false, page: 0 };
let edited = false, pageEdited = false, consumed: number | undefined;
let operation = 0;
const listeners = new Set<() => void>();
const owner = (value: Pick<SearchSessionView, 'project' | 'selectionEpoch'>) => `${value.project}:${value.selectionEpoch}`;
const commit = (next: SearchSessionView) => { current = next; for (const listener of listeners) { listener(); } };
export const searchSnapshot = () => current;
export function selectSearchProject(project = '', selectionEpoch = 0): void {
    if (owner(current) === owner({ project, selectionEpoch })) { return; }
    edited = false; pageEdited = false; consumed = undefined; operation++;
    commit({ project, selectionEpoch, form: { ...DEFAULT_SEARCH }, busy: false, page: 0 });
}
export function acceptSearchSession(next: SearchSessionView): void {
    if (owner(next) === owner(current)) { commit({ ...next, form: edited ? current.form : next.form, page: pageEdited ? current.page : next.page }); }
}
export function editSearch(form: SearchForm): void {
    edited = true; commit({ ...current, form });
    if (current.project) { notify('search.form', { ...form, project: current.project, selectionEpoch: current.selectionEpoch }); }
}
export function beginSearch(form: SearchForm): string {
    pageEdited = false;
    editSearch(form); commit({ ...current, busy: true, error: undefined, result: undefined, page: 0 });
    return `${owner(current)}:${++operation}`;
}
export function searchPage<T>(hits: readonly T[], page: number): readonly T[] { return hits.slice(page * 20, (page + 1) * 20); }
export function editSearchPage(page: number): void {
    const last = Math.max(0, Math.ceil((current.result?.hits.length ?? 0) / 20) - 1);
    page = Math.min(last, Math.max(0, Math.floor(page)));
    pageEdited = true; commit({ ...current, page });
    if (current.project) { notify('search.page', { project: current.project, selectionEpoch: current.selectionEpoch, page }); }
}
export function failSearch(expected: string, error: string): void {
    if (`${owner(current)}:${operation}` === expected) { commit({ ...current, busy: false, error, result: undefined }); }
}
export function consumeSearchInput(serial: number | undefined): boolean {
    if (serial === undefined || serial === consumed) { return false; }
    consumed = serial; return true;
}
export function useSearchSession(): SearchSessionView {
    const [, render] = useState(0);
    useEffect(() => { const listener = () => render(value => value + 1); listeners.add(listener); return () => { listeners.delete(listener); }; }, []);
    return current;
}
