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
