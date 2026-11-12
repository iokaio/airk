// SPDX-License-Identifier: Apache-2.0
// The window search scope in the Overview and Search headers (U77): what this
// window searches, with Clear (scope_release through `indexing.scope.apply`
// with nothing selected) and, when a read was refused as out of date, Reselect.
import { useState } from 'preact/hooks';
import type { ExplorerViewState } from '../explorerContracts';
import type { Route } from '../explorerRoutes';
import { request } from './api';
import { CLEAR_SCOPE } from './indexing';

export function ScopeBanner(props: { state: ExplorerViewState; go(route: Route): void }) {
    const [busy, setBusy] = useState(false), [error, setError] = useState('');
    const scope = props.state.searchScope;
    if (!props.state.trusted || !scope?.available || !scope.active && !scope.outOfDate) { return null; }
    const clear = async () => {
        setBusy(true); setError('');
        try { await request('indexing.scope.apply', CLEAR_SCOPE); }
        catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(false); }
    };
    const reselect = () => props.go({ workspace: 'indexing', tab: 'window' });
    return <div class="scope-banner callout" role="status">
        {scope.outOfDate
            ? <p>Search scope out of date — reselect.{scope.label ? <> Last chosen: <strong>{scope.label}</strong>.</> : null}</p>
            : <p>Searching: <strong>{scope.label}</strong></p>}
        <span class="actions">
            {scope.outOfDate && <button type="button" disabled={busy} onClick={reselect}>Reselect</button>}
            {!scope.outOfDate && <button type="button" class="ghost" disabled={busy} onClick={reselect}>Change</button>}
            {scope.active && <button type="button" class="ghost" disabled={busy} onClick={() => { void clear(); }}>Clear</button>}
        </span>
        {error && <p role="alert">{error}</p>}
    </div>;
}
