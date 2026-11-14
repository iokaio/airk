// SPDX-License-Identifier: Apache-2.0
// Uses this webview's existing Preact singleton and closed C9 read bridge.
import { h, type JSX, type VNode } from 'preact';
import { useLayoutEffect, useRef, useState } from 'preact/hooks';
import { pilotPresentation, renderPilotNode } from '../../../ui/src/pilotPresentation';
import type { PilotObservation, PilotRead } from '../../../ui/src/pilotContract';
import type { ExplorerViewState } from '../explorerContracts';
import { request } from './api';

export function SharedPilotPane(props: { state: ExplorerViewState; read: PilotRead }): JSX.Element | null {
    const [observation, setObservation] = useState<PilotObservation | null>(null);
    const [busy, setBusy] = useState(false), [error, setError] = useState('');
    const mounted = useRef(true), sequence = useRef(0);
    useLayoutEffect(() => { mounted.current = true; return () => { mounted.current = false; sequence.current++; }; }, []);
    const available = props.state.pilot?.available === true && props.state.trusted && props.state.nativeExecutionEnabled !== false
        && (props.state.connection === 'ready' || props.state.connection === 'degraded');
    const read = async (): Promise<void> => {
        const mine = ++sequence.current;
        setBusy(true); setError('');
        try {
            const result = await request<PilotObservation>('pilot.read', { epoch: props.state.pilot?.epoch ?? 0, read: props.read });
            if (mounted.current && mine === sequence.current) setObservation(result);
        } catch {
            if (mounted.current && mine === sequence.current) setError('The preview could not be read. Check the current host and service state.');
        } finally { if (mounted.current && mine === sequence.current) setBusy(false); }
    };
    if (!props.state.pilot) return null;
    const shown = observation && (!available || observation.request.epoch !== props.state.pilot.epoch
        || JSON.stringify(observation.request.read) !== JSON.stringify(props.read))
        ? { ...observation, applicability: 'superseded' as const } : observation;
    const disabled = busy || !available || props.read.kind === 'usage-json' && !/^[0-9a-f]{64}$/.test(props.read.binding);
    return <details class="shared-pilot"><summary>Shared view preview</summary>
        <p>This optional read preview keeps the existing view available. Nothing is changed or saved.</p>
        {props.read.kind === 'usage-json' && <p>The native report currently supports 1–30 days; larger periods retain the native refusal.</p>}
        <button type="button" disabled={disabled} onClick={() => void read()}>Read shared preview</button>
        {busy && <p role="status">Reading preview…</p>}{error && <p role="alert">{error}</p>}
        {shown && renderPilotNode<VNode>((tag, children) => h(tag, null, ...children), pilotPresentation(shown, 'VS Code window'))}
    </details>;
}
