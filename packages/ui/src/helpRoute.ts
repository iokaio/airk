// SPDX-License-Identifier: Apache-2.0
// Inert Help navigation only: never a tool request, file path or authority token.
export type HelpLocation = { query: string; guide: string | null; tool: string };
const id = (value: string | null): string | null => value !== null && /^[a-z0-9][a-z0-9._-]{0,127}$/.test(value) ? value : null;
export function parseHelpLocation(params: URLSearchParams | undefined): HelpLocation {
  return { query: (params?.get('q') ?? '').slice(0, 256), guide: id(params?.get('guide') ?? null), tool: id(params?.get('tool') ?? null) ?? '' };
}
export function formatHelpLocation(location: HelpLocation): URLSearchParams {
  const params = new URLSearchParams();
  if (location.query) params.set('q', location.query.slice(0, 256));
  if (id(location.guide)) params.set('guide', location.guide!);
  if (id(location.tool)) params.set('tool', location.tool);
  return params;
}

/** Shared task labels and inert guide destinations; selecting one never starts an operation. */
export const HELP_STARTING_TASKS = [
  ['indexing.library', 'Create an index'], ['management.agents', 'Connect an agent'], ['recovering-errors', 'Find missing results'],
] as const;
/** Explorer's native guide namespace is retained; exact local guidance takes precedence. */
export function helpStartingTopic<T extends {id:string}>(topics:readonly T[], guide:typeof HELP_STARTING_TASKS[number][0]):T|undefined {
  const local=topics.filter(topic=>topic.id===guide);
  if(local.length)return local.length===1?local[0]:undefined;
  const engine=topics.filter(topic=>topic.id===`engine.${guide}`);
  return engine.length===1?engine[0]:undefined;
}
