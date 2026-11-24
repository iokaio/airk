// SPDX-License-Identifier: Apache-2.0
// Shared management presentation runs in the bundled webview. Only the trusted
// extension host can perform the closed native Admin operation calls.
import { useContext, useEffect, useRef, useState } from 'preact/hooks';
import { Activity, Agents, IndexLibrary, Logs, Overview, PageVisible, PreparedActions, Recovery, Search, ServiceWorkspace, Storage, Usage, type PageId } from '../../../ui/src/management';
import { adoptNativeSession, onSessionChange } from '../../../admin/src/session';
import { parseEnvelope } from '../../../admin/src/envelope';
import { createNativeSessionLifetime, type NativeSessionLifetime } from '../../../ui/src/nativeSessionLifetime';
import type { RetainedPreparation } from '../../../ui/src/apiPort';
import type { AdminUiObservation } from '../adminUiContract';
import type { ExplorerViewState } from '../explorerContracts';
import { onHost, request } from './api';
import 'airk-shared-admin-css';

let captured: { epoch: number; lifetime: NativeSessionLifetime } | undefined;
let currentEpoch: number | undefined;
let closedByUser = false;
onHost(message => {
    if (message.type === 'admin.ended' && captured?.epoch === message.epoch) { captured.lifetime.invalidate(); return; }
    if (message.type !== 'state') return;
    currentEpoch = message.state.adminManagement?.available ? message.state.adminManagement.epoch : undefined;
    // A view/generation change revokes the renderer, preserving native-owned
    // custody for readonly recapture. Trust/native revocation closes the private
    // host owner directly; only explicit Close ends it from this renderer.
    if (captured && captured.epoch !== currentEpoch) { captured.lifetime.invalidate(); captured = undefined; }
});

import { ManagementNavigation, type AdminLink } from './adminLinks';

export function SharedManagement(props: { state: ExplorerViewState; page?: PageId; target?: { link: AdminLink; serial: number }; active?: boolean; onUseScope?(id: string): void; onPageChange?: (page: PageId) => void }) {
    const epoch = props.state.adminManagement?.epoch;
    const available = props.state.adminManagement?.available === true;
    const visited = useRef(false);
    if (props.active !== false) visited.current = true;
    const started = visited.current;
    const [owner, setOwner] = useState<NativeSessionLifetime | null>(null);
    const [error, setError] = useState('');
    const [reopen, setReopen] = useState(0);
    useEffect(() => onSessionChange(session => { if (session === null) setOwner(null); }), []);
    useEffect(() => {
        let active = true;
        if (!started || !available || epoch === undefined) { setOwner(null); return; }
        if (closedByUser) { setOwner(null); return; }
        currentEpoch = epoch;
        const open = async () => {
            if (!captured || captured.epoch !== epoch || !captured.lifetime.isOpen()) {
                const opened = await request<{ epoch: number; serviceInstance: string; pending: RetainedPreparation[] }>('admin.session', { epoch });
                if (!active || currentEpoch !== epoch || opened.epoch !== epoch) return;
                const lifetime = createNativeSessionLifetime({ kind: 'native', serviceInstance: opened.serviceInstance, preparations: opened.pending,
                    isCurrent: () => currentEpoch === epoch,
                    async call(operation, body) {
                        const outcome = await request<AdminUiObservation>('admin.call', { epoch, operation, payload: { body } });
                        // Late full outcomes remain historical evidence; their old
                        // context can never reopen the captured UI lifetime.
                        if (!outcome.current) lifetime.invalidate();
                        return 'transport' in outcome.reply ? { kind: 'transport', status: outcome.reply.httpStatus, message: outcome.reply.message }
                            : parseEnvelope(outcome.reply.httpStatus, outcome.reply.body);
                    },
                    async close() { closedByUser = true; await request('admin.close', { epoch }); },
                });
                captured = { epoch, lifetime };
            }
            if (active && captured?.epoch === epoch) { adoptNativeSession(captured.lifetime); setOwner(captured.lifetime); setError(''); }
        };
        void open().catch(failure => { if (active) { setOwner(null); setError(failure instanceof Error ? failure.message : String(failure)); } });
        // Navigation unmount does not revoke the captured host session/custody.
        return () => { active = false; };
    }, [epoch, available, reopen, started]);
    if (!started) return null;
    const canStart = props.page === 'service' && props.state.connection === 'disconnected'
        && props.state.trusted && props.state.nativeExecutionEnabled !== false;
    const startService = () => { void request('manage.command', { command: 'airk.serviceStart' })
        .catch(failure => setError(failure instanceof Error ? failure.message : String(failure))); };
    if (!available) return <section>
        <p>Management requires a connected AIRK Service, workspace trust and enabled native execution.</p>
        {canStart && <button type="button" onClick={startService}>Start AIRK Service</button>}
        {error && <p role="alert">{error}</p>}
    </section>;
    if (!owner) return <section><p>{error || (closedByUser || captured && !captured.lifetime.isOpen() ? 'The management connection ended. Reconnect to review any pending actions.' : 'Opening native management…')}</p>
        {canStart && <button type="button" onClick={startService}>Start AIRK Service</button>}
        {closedByUser || error || captured && !captured.lifetime.isOpen() ? <button type="button" onClick={() => { closedByUser = false; setError(''); setReopen(value => value + 1); }}>Open native management</button> : null}</section>;
    const close = () => {
        closedByUser = true;
        owner.invalidate();
        setOwner(null);
        void owner.close().catch(() => setError('Connection close could not be confirmed. Verify the service before reconnecting.'));
    };
    return <div class="airk-shared-management">
        <details><summary>Management connection</summary>
            <p>Service instance: <code>{owner.session.serviceInstance}</code></p>
            <button type="button" class="secondary" onClick={close}>Close management session</button>
        </details>
        <ManagementViews key={epoch} page={props.page} active={props.active} target={props.target} onUseScope={props.onUseScope}
            onActivity={() => props.onPageChange?.('activity')} onLifecycle={close} />
    </div>;
}

function ManagementViews(props: {
    page?: PageId;
    active?: boolean;
    target?: { link: AdminLink; serial: number };
    onUseScope?(id: string): void;
    onActivity(): void;
    onLifecycle(): void;
}) {
    const navigate = useContext(ManagementNavigation);
    const visited = useRef(new Set<PageId>());
    if (props.page) visited.current.add(props.page);
    const [prefill, setPrefill] = useState({ params: new URLSearchParams(), seq: 0 });
    useEffect(() => {
        const target = props.target;
        if (!target) return;
        const { folder, index, set } = target.link;
        const params = new URLSearchParams();
        if (folder !== undefined) params.set('root', folder);
        if (index !== undefined) params.set('index', index);
        if (set !== undefined) params.set('set', set);
        setPrefill({ params, seq: target.serial });
    }, [props.target]);
    const pages = {
        overview: <Overview />,
        'index-library': <IndexLibrary {...prefill} onActivity={props.onActivity} onUseScope={props.onUseScope} />,
        activity: <Activity onLogs={logTarget => navigate({ page: 'logs', logTarget })} />,
        agents: <Agents />,
        search: <Search />,
        storage: <Storage />,
        recovery: <Recovery />,
        service: <ServiceWorkspace editor onLifecycle={props.onLifecycle} />,
        logs: <Logs target={props.target?.link.logTarget} />,
        usage: <Usage />,
    };
    return <div class="layout">
        <div class="management-content">
            <PreparedActions />
            {(Object.keys(pages) as (keyof typeof pages)[]).map(id => visited.current.has(id) ?
                <section key={id} hidden={id !== props.page}>
                    <PageVisible.Provider value={id === props.page && props.active !== false}>
                        {pages[id]}
                    </PageVisible.Provider>
                </section> : null)}
        </div>
    </div>;
}
