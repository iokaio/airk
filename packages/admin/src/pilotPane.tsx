// SPDX-License-Identifier: Apache-2.0
// Opt-in read preview; existing rich Usage/Help workflows retain their owners.
import { h, type JSX, type VNode } from 'preact';
import { useLayoutEffect, useRef, useState } from 'preact/hooks';
import { createBrowserPilot } from '../../ui/src/browserPilot';
import { pilotPresentation, renderPilotNode } from '../../ui/src/pilotPresentation';
import type { PilotContext, PilotObservation, PilotRead } from '../../ui/src/pilotContract';
import { currentSession } from './session';

export function SharedPilotPane(props: { read: PilotRead }): JSX.Element {
  const session = currentSession(), readKey = JSON.stringify(props.read);
  const owner = useRef({ session, readKey, epoch: 0 });
  if (owner.current.session !== session || owner.current.readKey !== readKey) owner.current = { session, readKey, epoch: owner.current.epoch + 1 };
  const context = useRef<PilotContext>({ host: 'browser', epoch: 0, ready: false, trusted: true, nativeEnabled: true });
  context.current = { host: 'browser', epoch: owner.current.epoch, ready: session !== null, trusted: true, nativeEnabled: true,
    ...(props.read.kind === 'usage-json' ? { usageBinding: props.read.binding } : {}) };
  const reader = useRef<ReturnType<typeof createBrowserPilot>>();
  reader.current ??= createBrowserPilot(() => ({ ...context.current, ready: currentSession() !== null }));
  const [observation, setObservation] = useState<PilotObservation | null>(null);
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  const mounted = useRef(true), sequence = useRef(0);
  useLayoutEffect(() => { mounted.current = true; return () => { mounted.current = false; sequence.current++; }; }, []);
  const read = async (): Promise<void> => {
    const mine = ++sequence.current;
    setBusy(true); setError('');
    try {
      const result = await reader.current!({ v: 1, type: 'pilot.read', requestId: `preview_${mine}`, epoch: context.current.epoch, read: props.read });
      if (mounted.current && mine === sequence.current) setObservation(result);
    } catch {
      if (mounted.current && mine === sequence.current) setError('The preview could not be read. Check the current session and service state.');
    } finally { if (mounted.current && mine === sequence.current) setBusy(false); }
  };
  const shown = observation && (observation.request.epoch !== context.current.epoch || session === null)
    ? { ...observation, applicability: 'superseded' as const } : observation;
  const disabled = busy || session === null || props.read.kind === 'usage-json' && !/^[0-9a-f]{64}$/.test(props.read.binding);
  return <details class="shared-pilot"><summary>Shared view preview</summary>
    <p>This optional read preview keeps the existing view available. Nothing is changed or saved.</p>
    {props.read.kind === 'usage-json' ? <p>The native report currently supports 1–30 days; larger periods retain the native refusal.</p> : null}
    <button type="button" disabled={disabled} onClick={() => void read()}>Read shared preview</button>
    {busy ? <p role="status">Reading preview…</p> : null}{error ? <p role="alert">{error}</p> : null}
    {shown ? renderPilotNode<VNode>((tag, children) => h(tag, null, ...children), pilotPresentation(shown, 'Browser Admin session')) : null}
  </details>;
}
