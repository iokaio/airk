// SPDX-License-Identifier: Apache-2.0
import { useEffect, useRef, useState } from 'preact/hooks';
import { OpenSourceResult, PassageView, SearchMode, SearchForm } from '../explorerContracts';
import { RECIPE_SCHEMA, SearchRecipe } from '../explorerStrategies';
import { HostFailure, request } from './api';
import { countLine, shortToken } from './format';
import type { ScreenProps } from './workspaces';
import { FileBrowser, type LibraryFile } from './fileBrowser';
import { currentSelection } from './strategyStore';
import { dialogFocus } from './components';
import { beginSearch, consumeSearchInput, editSearch, editSearchPage, failSearch, searchPage, useSearchSession } from './searchStore';

type Form = SearchForm;

export function SearchScreen(props: ScreenProps) {
    const status = props.state.status;
    const { form, result, error, busy, page } = useSearchSession();
    const [passages, setPassages] = useState<Record<string, PassageView>>({});
    const [opened, setOpened] = useState<Record<string, OpenSourceResult | string>>({});
    const input = useRef<HTMLInputElement>(null);
    const vectorsOff = status?.vectorsEnabled === false || status?.semantic === 'disabled';
    const vectorReason = vectorsOff ? 'Semantic search is turned off for this project. Enable vectors in Management › Indexing.'
        : status?.semantic === 'pending' ? 'The semantic index is building; semantic results may be empty or unavailable until vectors are published.'
        : status?.semantic === 'partial' ? 'Semantic coverage is partial. Search uses the vectors published so far.'
        : status?.semantic === 'failed' ? 'Semantic indexing failed. Check Management › Indexing for the failure; semantic results may be unavailable or incomplete.' : '';
    const canSearch = props.state.connection === 'ready' || props.state.connection === 'degraded';

    const run = async (next: Form) => {
        if (!next.query.trim()) { return; }
        const owner = beginSearch(next); setPassages({}); setOpened({});
        try {
            await request('search.run', { query: next.query, mode: next.mode, limit: next.limit, maxBytes: next.maxBytes,
                fullChunks: next.fullChunks, ...(next.pathPrefix.trim() ? { pathPrefix: next.pathPrefix.trim() } : {}) });
        } catch (failure) {
            if (failure instanceof HostFailure && failure.error.code === 'REQUEST_SUPERSEDED') { return; }
            failSearch(owner, failure instanceof Error ? failure.message : String(failure));
        }
    };

    useEffect(() => {
        if (!consumeSearchInput(props.incoming?.serial)) { input.current?.focus(); return; }
        const recipe = props.incoming?.recipe;
        if (recipe) {
            const next: Form = { query: recipe.query, mode: recipe.mode, pathPrefix: recipe.pathPrefix ?? '', limit: recipe.limit, maxBytes: recipe.maxBytes, fullChunks: recipe.fullChunks };
            editSearch(next);
            // An applied agent draft only fills the form; the user decides whether to run it.
            if (props.incoming?.run !== false) { void run(next); }
        } else if (props.incoming?.query) { const next = { ...form, query: props.incoming.query }; editSearch(next); void run(next); }
        else { input.current?.focus(); }
    }, [props.incoming?.serial]);

    const expand = async (ref: string) => {
        try { const passage = await request<PassageView>('evidence.expand', { ref, before: 5, after: 5 }); setPassages(prior => ({ ...prior, [ref]: passage })); }
        catch (failure) { setOpened(prior => ({ ...prior, [ref]: failure instanceof Error ? failure.message : String(failure) })); }
    };
    const open = async (ref: string) => {
        try { const outcome = await request<OpenSourceResult>('source.open', { ref }); setOpened(prior => ({ ...prior, [ref]: outcome })); }
        catch (failure) { setOpened(prior => ({ ...prior, [ref]: failure instanceof Error ? failure.message : String(failure) })); }
    };
    const field = <K extends keyof Form>(key: K, value: Form[K]) => editSearch({ ...form, [key]: value });

    return (
        <div class="search">
            <p class="muted">Search this project’s published index using the query and path scope below. The source file selected in the header does not limit this search. On a result, Inspect opens its indexed record and saved-file comparison in Evidence › Inspector; Open source opens its cited lines in the editor.</p>
            <form class="search-form" onSubmit={event => { event.preventDefault(); void run(form); }}>
                <label class="grow">Query
                    <input ref={input} value={form.query} maxLength={4096} required placeholder={form.mode === 'lexical' ? 'Words that must all appear' : 'Describe what you want to find'}
                        onInput={event => field('query', (event.target as HTMLInputElement).value)} />
                </label>
                <label>Mode
                    <select value={form.mode} onChange={event => field('mode', (event.target as HTMLSelectElement).value as SearchMode)}>
                        <option value="policy">Collection policy</option>
                        <option value="lexical">Lexical (all words)</option>
                        <option value="vector" disabled={vectorsOff}>Semantic (by meaning)</option>
                        <option value="hybrid" disabled={vectorsOff}>Hybrid (lexical + semantic)</option>
                    </select>
                </label>
                <label>Path scope
                    <input value={form.pathPrefix} maxLength={1024} placeholder="e.g. src/auth/" onInput={event => field('pathPrefix', (event.target as HTMLInputElement).value)} />
                </label>
                <label>Limit
                    <input type="number" min={1} max={100} value={form.limit} onInput={event => field('limit', Math.min(100, Math.max(1, Number((event.target as HTMLInputElement).value) || 1)))} />
                </label>
                <label>Bytes
                    <select value={String(form.maxBytes)} onChange={event => field('maxBytes', Number((event.target as HTMLSelectElement).value))}>
                        <option value="8192">8 KB</option><option value="16384">16 KB</option><option value="32768">32 KB</option><option value="65536">64 KB</option>
                    </select>
                </label>
                <label class="check"><input type="checkbox" checked={form.fullChunks} onChange={event => field('fullChunks', (event.target as HTMLInputElement).checked)} /> Full chunks</label>
                <button type="submit" disabled={busy || !canSearch}>{busy ? 'Searching…' : 'Search'}</button>
            </form>
            {form.mode === 'vector' && <p class="muted">Describe a concept or behavior in natural language. Semantic search ranks indexed passages by embedding similarity; the exact query words need not appear.</p>}
            {vectorReason && <p class="muted">{vectorReason}</p>}
            <SaveRecipe form={form} />
            {!canSearch && <p class="callout">Search needs a connected AIRK service and a published index ({props.state.connection}).</p>}
            {error && <p class="callout" role="alert">{error}</p>}
            {result && (
                <section aria-label="Search results">
                    <p class="muted">Results for “{result.query}”</p>
                    {result.publication && status?.publication && result.publication !== status.publication && <p class="callout">These retained results are from an earlier publication. Run Search again to use the current index.</p>}
                    <p class="result-meta">
                        <strong>{countLine(result.returned, result.limit, result.truncated)}</strong>
                        {' · '}mode: {result.appliedMode === 'vector' ? 'semantic' : result.appliedMode}{result.semantic && <> · semantic coverage: {result.semantic}</>}{' · '}publication <span title={result.publication}>{shortToken(result.publication)}</span>
                    </p>
                    {result.warnings.map(warning => <p key={warning} class="muted">{warning}</p>)}
                    {result.hits.length > 0 && <nav class="actions pagination" aria-label="Search result pages">
                        <button class="ghost" disabled={page === 0} onClick={() => editSearchPage(page - 1)}>Previous</button>
                        <span role="status" aria-live="polite" aria-atomic="true">{page * 20 + 1}–{Math.min((page + 1) * 20, result.hits.length)} of {result.hits.length} results · Page {page + 1} of {Math.ceil(result.hits.length / 20)}</span>
                        <button class="ghost" disabled={(page + 1) * 20 >= result.hits.length} onClick={() => editSearchPage(page + 1)}>Next</button>
                    </nav>}
                    {!result.hits.length && <p class="callout">No indexed evidence matched. That is not proof the text is absent from unindexed or changed files.</p>}
                    <ol class="hits" start={page * 20 + 1}>
                        {searchPage(result.hits, page).map(hit => {
                            const outcome = opened[hit.ref];
                            const passage = passages[hit.ref];
                            return (
                                <li key={hit.ref} class="hit card">
                                    <div class="hit-head">
                                        <code class="path">{hit.path}:{hit.startLine}{hit.endLine !== hit.startLine ? `–${hit.endLine}` : ''}</code>
                                        <span class="tag" title="E1: indexed text — not a resolved binding or coverage">{hit.evidenceClass}</span>
                                        {hit.location && <span class="tag" title="Original document location">{hit.location}</span>}
                                        {hit.truncated && <span class="tag">truncated</span>}
                                        <span class="spacer" />
                                        <button type="button" class="ghost" onClick={() => { void expand(hit.ref); }}>Expand</button>
                                        <button type="button" class="ghost" onClick={() => props.inspect(hit.ref)}>Inspect</button>
                                        <button type="button" onClick={() => { void open(hit.ref); }}>Open source</button>
                                    </div>
                                    <pre class="excerpt">{passage ? passage.text : hit.text}</pre>
                                    {passage && <p class="muted">Indexed lines {passage.startLine}–{passage.endLine}{passage.truncated ? ' · truncated' : ''}</p>}
                                    {outcome && <p class={typeof outcome === 'string' || outcome.live !== 'matches_index' ? 'callout' : 'muted'} role="status">
                                        {typeof outcome === 'string' ? outcome : outcome.message}</p>}
                                </li>
                            );
                        })}
                    </ol>
                    <p class="muted">A matched phrase is not a verified implementation. Open the source to investigate.</p>
                </section>
            )}
        </div>
    );
}

const recipeOf = (name: string, form: Form): SearchRecipe => ({ schema: RECIPE_SCHEMA, name: name.trim(), query: form.query, mode: form.mode,
    ...(form.pathPrefix.trim() ? { pathPrefix: form.pathPrefix.trim() } : {}), limit: form.limit, maxBytes: form.maxBytes, fullChunks: form.fullChunks });

/** Save the current form as a named recipe in this workspace. Saving never runs anything. */
function SaveRecipe(props: { form: Form }) {
    const [name, setName] = useState('');
    const [status, setStatus] = useState<string>();
    const save = async () => {
        try { await request<SearchRecipe[]>('recipes.save', { recipe: recipeOf(name, props.form) as unknown as Record<string, unknown> }); setStatus(`Saved search "${name.trim()}".`); setName(''); }
        catch (failure) { setStatus(failure instanceof Error ? failure.message : String(failure)); }
    };
    return (
        <form class="search-form recipe-save" onSubmit={event => { event.preventDefault(); void save(); }}>
            <label>Search name<input value={name} maxLength={80} placeholder="Name this search" onInput={event => setName((event.target as HTMLInputElement).value)} /></label>
            <button type="submit" class="ghost" disabled={!name.trim() || !props.form.query.trim()}>Save</button>
            {status && <span class="muted" role="status">{status}</span>}
        </form>
    );
}

/** Search › Recipes: saved argument sets for this workspace (airk.explorer.search-recipe/1). */
export function RecipesScreen(props: ScreenProps) {
    const [recipes, setRecipes] = useState<SearchRecipe[]>();
    const [error, setError] = useState<string>();
    const [notice, setNotice] = useState<string>(), [busy, setBusy] = useState(false);
    const [pending, setPending] = useState<{ recipe: SearchRecipe; expected: string; owner: string | undefined }>();
    const root = useRef<HTMLDivElement>(null), epoch = useRef(0);
    useEffect(() => dialogFocus(root.current?.querySelector<HTMLElement>('[role="alertdialog"]') ?? null, () => setPending(undefined)), [pending]);
    useEffect(() => () => { epoch.current++; }, []);
    useEffect(() => { request<SearchRecipe[]>('recipes.list', {}).then(setRecipes, failure => setError(failure instanceof Error ? failure.message : String(failure))); }, []);
    const remove = async (name: string) => {
        epoch.current++; setPending(undefined);
        try { setRecipes(await request<SearchRecipe[]>('recipes.delete', { name })); } catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
    };
    const loadFile = async (file?: LibraryFile) => {
        const generation = ++epoch.current, owner = currentSelection();
        setBusy(true); setError(undefined); setPending(undefined);
        try {
            const recipe = file ? await request<SearchRecipe>('files.load', { kind: 'recipes', ...file }) : await request<SearchRecipe | null>('recipes.import', {});
            if (recipe && generation === epoch.current && owner === currentSelection()) { setPending({ recipe, expected: JSON.stringify(recipes?.find(entry => entry.name === recipe.name) ?? null), owner }); }
        } catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(false); }
    };
    const acceptImport = async () => {
        if (!pending || pending.owner !== currentSelection()) { setPending(undefined); return; }
        setBusy(true); setError(undefined);
        try {
            setRecipes(await request<SearchRecipe[]>('recipes.save', { recipe: pending.recipe as unknown as Record<string, unknown>, expected: pending.expected }));
            setNotice(`Imported "${pending.recipe.name}". Choose Run explicitly to search.`); setPending(undefined);
        } catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(false); }
    };
    const exportFile = async (recipe: SearchRecipe) => {
        setBusy(true); setError(undefined);
        try {
            const reply = await request<{ saved: boolean; path?: string; reason?: string }>('recipes.export', { recipe: recipe as unknown as Record<string, unknown> });
            setNotice(reply.saved ? `Exported ${reply.path}.` : reply.reason);
        } catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(false); }
    };
    if (!recipes && !error) { return <p class="muted">Loading saved searches…</p>; }
    return (
        <div class="recipes" ref={root}>
            <h2>Saved Searches</h2>
            <p class="muted">Saved searches keep the query and search options for this workspace. Run fills the Search form and submits it; saving never changes the index.</p>
            <div class="actions"><button class="ghost" disabled={busy || !props.state.trusted} onClick={() => void loadFile()}>Import search…</button></div>
            {error && <p class="callout" role="alert">{error}</p>}{notice && <p class="muted" role="status">{notice}</p>}
            {pending && <section class="card" role="alertdialog" aria-label="Import saved search"><h2>Import {pending.recipe.name}?</h2>
                <p>{pending.expected === 'null' ? 'Add this search to this workspace.' : 'Replace the same-named saved search.'} Loading never runs a search.</p>
                <pre class="excerpt">{pending.recipe.query}</pre><p class="muted">Mode: {pending.recipe.mode} · Scope: {pending.recipe.pathPrefix ?? '(all)'}</p>
                <div class="actions"><button disabled={busy} onClick={() => void acceptImport()}>{pending.expected === 'null' ? 'Add search' : 'Replace saved search'}</button>
                    <button class="ghost" disabled={busy} onClick={() => setPending(undefined)}>Cancel import</button></div></section>}
            {!recipes?.length ? <p class="muted">No saved searches yet. Enter a search name and choose <strong>Save</strong> on Search › Query.</p> : (
                <div class="table-wrap"><table class="grid">
                    <thead><tr><th>Name</th><th>Query</th><th>Mode</th><th>Path scope</th><th>Limit</th><th></th></tr></thead>
                    <tbody>{recipes.map(recipe => (
                        <tr key={recipe.name}>
                            <td>{recipe.name}</td><td><code>{recipe.query}</code></td><td>{recipe.mode === 'policy' ? 'collection policy' : recipe.mode}</td>
                            <td>{recipe.pathPrefix ?? '(all)'}</td><td>{recipe.limit}</td>
                            <td class="actions"><button disabled={busy} onClick={() => props.runRecipe(recipe)}>Run</button> <button class="ghost" disabled={busy} onClick={() => void exportFile(recipe)}>Export JSON</button> <button class="ghost" disabled={busy} onClick={() => void remove(recipe.name)}>Delete</button></td>
                        </tr>
                    ))}</tbody>
                </table></div>
            )}
            <FileBrowser kind="recipes" trusted={props.state.trusted} disabled={busy} load={loadFile} />
        </div>
    );
}
