// SPDX-License-Identifier: Apache-2.0
import type { JSX } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { acknowledgePreparation, onPreparationChange, pendingPreparations, recoverPreparation, type CallResult } from './api';
import { currentSession } from './session';
import { JsonBlock, Text } from './render';

/** Restored actions expose recovery only. Opening this panel does not read a
 * receipt, submit an action, acknowledge a result, or generate a native key. */
export function PreparedActions(): JSX.Element | null {
  const session = currentSession();
  const [, changed] = useState(0);
  const [observed, setObserved] = useState<{ handle: string; result: CallResult } | null>(null);
  useEffect(() => onPreparationChange(() => changed(value => value + 1)), [session]);
  const entries = pendingPreparations();
  if (!entries.length && !observed) return null;
  return <section class="panel" aria-label="Prepared action recovery">
    <h2>Original prepared actions</h2>
    <p>Read the original outcome before starting another action. Recovery never resubmits it. A preparation that is still ready has not been submitted; a fresh session is needed to abandon it.</p>
    {entries.map(entry => <article key={entry.preparation.preparation}>
      <h3><Text value={entry.operation} /></h3>
      <p>{entry.pending ? 'Reading or submitting the original action…' : entry.state === 'known' ? 'Complete result retained; native acknowledgment was not confirmed.'
        : entry.state === 'ready' ? 'Prepared only; no mutation submitted.' : 'Original outcome unresolved.'}</p>
      <button type="button" disabled={entry.pending} onClick={() => void (entry.state === 'known'
        ? acknowledgePreparation(entry.preparation.preparation) : recoverPreparation(entry.preparation.preparation)).then(result => {
        if (currentSession() === session) setObserved({ handle: entry.preparation.preparation, result });
      })}>{entry.state === 'known' ? 'Retry outcome acknowledgment' : 'Read original outcome'}</button>
      <details><summary>Original public input and acknowledgment</summary><JsonBlock name="prepared-action.json" value={{ operation: entry.operation, body: entry.body, acknowledgment: entry.acknowledgment }} /></details>
    </article>)}
    {observed ? <JsonBlock name="prepared-action-outcome.json" value={observed.result} /> : null}
  </section>;
}
