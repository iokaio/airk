// SPDX-License-Identifier: Apache-2.0
// Native Core owns all model/configuration/task DTOs and readiness decisions.
import type { JSX } from 'preact';
import { useRef, useState } from 'preact/hooks';
import type { Enrollment, ManagementRequest, Status, Task } from '../../contracts-gen/arbiter';
import { isUnknownOutcome } from './api';
import { isCatalogId, OPS, type Operation } from './operations';
import { Field, Outcome, Panel, Text, useCall } from './render';

const operations = { status: OPS.arbiterStatus, configure: OPS.arbiterConfigure, provision: OPS.arbiterProvision,
  cancel: OPS.arbiterCancel, remove: OPS.arbiterRemove, recover: OPS.arbiterRecover } as const;
export function Arbiter(): JSX.Element {
  const [outcome, run] = useCall();
  const [status, setStatus] = useState<Status | null>(null);
  const [enabled, setEnabled] = useState(false), [profile, setProfile] = useState('');
  const [indexes, setIndexes] = useState<Enrollment[]>([]), [index, setIndex] = useState('');
  const [tasks, setTasks] = useState<Task[]>([]), [unresolved, setUnresolved] = useState(false);
  const [sending, setSending] = useState(false);
  const admitted = useRef(false);
  const send = async (request: ManagementRequest): Promise<void> => {
    if (admitted.current || request.operation !== 'status' && unresolved) return;
    admitted.current = true; setSending(true);
    try {
      const result = await run(operations[request.operation] as Operation, { arbiter: { request } });
      if (request.operation !== 'status') setUnresolved(isUnknownOutcome(result));
      if (result.kind !== 'ok') return;
      const native = result.envelope.result as { status?: unknown; observation?: Status };
      if (native?.status !== 'arbiter' || !native.observation || !Array.isArray(native.observation.profiles)
          || !Array.isArray(native.observation.configuration?.indexes)) return;
      const observed = native.observation;
      setStatus(observed); setEnabled(observed.configuration.enabled); setIndexes(observed.configuration.indexes);
      setProfile(observed.selected_profile ?? observed.configuration.selected_profile ?? '');
      // A status read permits review of a fresh revision; it never repeats the mutation.
      if (request.operation === 'status') setUnresolved(false);
    } finally { admitted.current = false; setSending(false); }
  };
  const availableTasks = [...new Set(status?.profiles.flatMap(entry => entry.tasks) ?? [])];
  const revision = status?.configuration.revision;
  return <Panel title="Optional local Arbiter">
    <p>Arbiter is off by default. Enabling it does not enroll any index. Index enrollment uses immutable index IDs and explicit tasks. MiniLM retrieval embeddings remain a separate index setting.</p>
    <button type="button" disabled={sending} onClick={() => void send({ operation: 'status' })}>Read Arbiter status</button>
    {status ? <>
      <p>Availability: <Text value={status.availability} />. <Text value={status.reason} /></p>
      <p>Selected profile: <Text value={status.selected_profile ?? 'none'} />. Active profile: <Text value={status.active_profile ?? 'none'} />. Queued: {status.queued}; running: {status.running}.</p>
      <label><input type="checkbox" checked={enabled} onChange={event => setEnabled(event.currentTarget.checked)} /> Enable optional local Arbiter</label>
      <h3>Local model profiles</h3>
      <p>Availability, download sizes and memory reservations below are native observations. A missing qualified profile cannot be provisioned.</p>
      {status.profiles.map(entry => <article key={entry.model}>
        <h4><Text value={entry.model} /></h4>
        <p><Text value={entry.availability} />: <Text value={entry.reason} />. Download: {entry.download_bytes ?? 'not reported'} bytes; memory reservation: {entry.memory_reservation_bytes ?? 'not reported'} bytes.</p>
        <label><input type="radio" name="arbiter-profile" checked={!!entry.profile && profile === entry.profile} disabled={!entry.profile}
          onChange={() => setProfile(entry.profile ?? '')} /> Select this profile</label>
        <div class="actions"><button type="button" disabled={sending || unresolved || !entry.profile || entry.availability === 'unqualified'} onClick={() => entry.profile && revision !== undefined && void send({ operation: 'provision', expected_revision: revision, profile: entry.profile })}>Provision verified profile</button>
          <button type="button" class="danger" disabled={sending || unresolved || !entry.profile} onClick={() => entry.profile && revision !== undefined && void send({ operation: 'remove', expected_revision: revision, profile: entry.profile })}>Remove local profile</button></div>
      </article>)}
      <h3>Explicit index enrollment</h3>
      {indexes.map(entry => <div class="row" key={entry.index_id}><Text value={entry.index_id} /><span>{entry.tasks.join(', ')}</span>
        <button type="button" class="secondary" onClick={() => setIndexes(prior => prior.filter(item => item.index_id !== entry.index_id))}>Remove enrollment</button></div>)}
      <Field label="Immutable index ID (from Indexes)" value={index} onInput={setIndex} invalid={index !== '' && !isCatalogId('idx_', index)} />
      {availableTasks.map(task => <label key={task}><input type="checkbox" checked={tasks.includes(task)} onChange={event => setTasks(prior => event.currentTarget.checked ? [...prior, task] : prior.filter(value => value !== task))} /><Text value={task} /></label>)}
      <button type="button" class="secondary" disabled={!isCatalogId('idx_', index) || !tasks.length || indexes.some(entry => entry.index_id === index)} onClick={() => { setIndexes(prior => [...prior, { index_id: index, tasks: [...tasks] }]); setIndex(''); setTasks([]); }}>Add explicit index enrollment</button>
      <p>Saving uses native revision {revision}. A concurrent change is refused; read status and review again.</p>
      <div class="actions"><button type="button" disabled={sending || unresolved} onClick={() => revision !== undefined && void send({ operation: 'configure', expected_revision: revision, enabled, indexes, selected_profile: profile || null })}>Save Arbiter configuration</button>
        <button type="button" class="secondary" disabled={sending || unresolved} onClick={() => revision !== undefined && void send({ operation: 'cancel', expected_revision: revision })}>Cancel model operation</button>
        <button type="button" class="secondary" disabled={sending || unresolved} onClick={() => revision !== undefined && void send({ operation: 'recover', expected_revision: revision })}>Recover model state</button></div>
    </> : <p>No native Arbiter status has been read. No profile is assumed installed, ready or active.</p>}
    {unresolved ? <p class="error">The original management outcome is unknown. Read current native status and review its revision before another change. No automatic retry occurs.</p> : null}
    <Outcome state={outcome} />
  </Panel>;
}
