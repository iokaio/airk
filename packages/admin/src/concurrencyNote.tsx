// SPDX-License-Identifier: Apache-2.0
// (U108) The query concurrency for the informational scope note, read through
// the same `service.resources` row the Service page loads, and the note itself.
import type { JSX } from 'preact';
import { useEffect } from 'preact/hooks';
import { body, OPS } from './operations';
import { resultOf, useCall } from './render';
import { queryConcurrencyOf } from './scopeConcurrency';

/** The configured query concurrency, read once per mount; null while unknown or unread. */
export function useQueryConcurrency(): number | null {
  const [resources, runResources] = useCall();
  useEffect(() => {
    void runResources(OPS.resources, body.resources());
  }, []);
  return queryConcurrencyOf(resultOf(resources));
}

/** The note as a status line; nothing when there is no note. */
export function ConcurrencyNote(props: { text: string | null }): JSX.Element | null {
  return props.text === null ? null : (
    <p class="concurrency-note" role="status">
      {props.text}
    </p>
  );
}
