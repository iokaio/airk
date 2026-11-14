// SPDX-License-Identifier: Apache-2.0
// Shared management presentation runs in the bundled webview. Only the trusted
// extension host can perform the closed native Admin operation calls.
import { useEffect, useRef, useState } from 'preact/hooks';
import { ManagementApp, type PageId } from '../../../ui/src/management';
import { adoptNativeSession } from '../../../admin/src/session';
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

export function SharedManagement(props: { state: ExplorerViewState; page?: PageId; active?: boolean; onPageChange?: (page: PageId) => void }) {
    const epoch = props.state.adminManagement?.epoch;
    const available = props.state.adminManagement?.available === true;
    const visited = useRef(false);
    if (props.active !== false) visited.current = true;
    const started = visited.current;
    const [owner, setOwner] = useState<NativeSessionLifetime | null>(null);
    const [error, setError] = useState('');
    const [reopen, setReopen] = useState(0);
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
    if (!available) return <p>Native management needs workspace trust and enabled native execution. Offline Help and editor evidence workflows remain available.</p>;
    if (!owner) return <section><p>{error || (closedByUser ? 'The captured management session is closed. Reopening creates a fresh native context; no original action is replayed.' : 'Opening native management…')}</p>
        {closedByUser || error ? <button type="button" onClick={() => { closedByUser = false; setError(''); setReopen(value => value + 1); }}>Open native management</button> : null}</section>;
    return <div class="airk-shared-management"><div class="actions" aria-label="Native management session">
        <span>Native management · {owner.session.serviceInstance.slice(0, 8)}</span>
        <button type="button" class="secondary" onClick={() => { closedByUser = true; owner.invalidate(); setOwner(null); void owner.close().catch(() => { setError('The original context close was not confirmed. Verify the native connection before opening another context.'); }); }}>Close management session</button>
        </div><ManagementApp key={epoch} initialGate={{ kind: 'none' }} embedded active={props.active} initialPage={props.page} onPageChange={props.onPageChange} /></div>;
}
