// SPDX-License-Identifier: Apache-2.0
// (U108) An informational note when a search scope or an index set covers more
// indexes than the service reads at once (`query_concurrency`, from the
// `service.resources` read). Not a limit: applying and saving work unchanged.
// Without an observed concurrency there is no note; it is never guessed.
//
// Pure and import-free so `node --test` runs it directly (test/).

/** A reported query concurrency: a positive safe integer, otherwise unknown. */
export function observedConcurrency(value: unknown): number | null {
  return typeof value === 'number' && Number.isSafeInteger(value) && value > 0 ? value : null;
}

/**
 * The configured query concurrency of a `service.resources` result
 * (ControlResponse `{status: "resources", observation: {configuration}}`).
 * A result with `configuration_error` reports a fallback, not the
 * configuration, so it stays unknown.
 */
export function queryConcurrencyOf(result: unknown): number | null {
  const rec = (value: unknown): Record<string, unknown> | null =>
    typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
  const response = rec(result);
  if (response === null || response.status !== 'resources' || response.configuration_error !== false) return null;
  return observedConcurrency(rec(rec(response.observation)?.configuration)?.query_concurrency);
}

/** Reading rounds for `indexes` at `concurrency` reads at a time; null when no note applies. */
export function concurrencyRounds(indexes: number, concurrency: number | null): number | null {
  const limit = observedConcurrency(concurrency);
  if (limit === null || !Number.isSafeInteger(indexes) || indexes <= limit) return null;
  return Math.ceil(indexes / limit);
}

const times = (rounds: number): string => (rounds === 2 ? 'twice' : rounds === 3 ? 'three times' : `${rounds} times`);

/**
 * The note's sentence, or null (n ≤ concurrency, or concurrency unknown).
 * `atLeast` marks a lower bound (index-set memberships not known).
 */
export function concurrencyNote(input: {
  indexes: number;
  concurrency: number | null;
  atLeast?: boolean;
  subject: 'scope' | 'set';
}): string | null {
  const rounds = concurrencyRounds(input.indexes, input.concurrency);
  if (rounds === null) return null;
  const count = `${input.atLeast === true ? 'at least ' : ''}${input.indexes} indexes`;
  const estimate = `${input.atLeast === true ? 'at least' : 'about'} ${times(rounds)} as long`;
  const machine = `this machine searches ${input.concurrency} at a time`;
  return input.subject === 'scope'
    ? `This scope searches ${count}; ${machine}, so searches take ${estimate}.`
    : `This index set will hold ${count}; ${machine}, so searching it takes ${estimate}.`;
}

/**
 * The index count of an index set after adding `adding`: its listed members
 * plus the new index once. Exact only when every membership page was read.
 */
export function setSizeAfterAdd(
  members: readonly string[],
  complete: boolean,
  adding: string,
): { count: number; exact: boolean } {
  const union = new Set(members);
  union.add(adding);
  return { count: union.size, exact: complete };
}

/**
 * The index count of a scope typed as index ids and index-set ids: exact for
 * indexes alone; with index sets their members are not known here, so the
 * distinct index ids are a lower bound.
 */
export function typedScopeCount(indexIds: readonly string[], projectIds: readonly string[]): { count: number; exact: boolean } {
  return { count: new Set(indexIds).size, exact: projectIds.length === 0 };
}
