// SPDX-License-Identifier: Apache-2.0
// Presentation destinations map to retained host routes; they grant no authority.
export const MAIN_DESTINATIONS = [
  { id: 'overview', title: 'Overview', browser: 'overview', editor: 'overview' },
  { id: 'search', title: 'Search', browser: 'search', editor: 'search/query' },
  { id: 'indexes', title: 'Indexes', browser: 'index-library', editor: 'indexing/library' },
  { id: 'activity', title: 'Activity', browser: 'activity', editor: 'manage/activity' },
  { id: 'agents', title: 'Agents', browser: 'agents', editor: 'manage/agents' },
  { id: 'service', title: 'Service', browser: 'service', editor: 'manage/service' },
  { id: 'help', title: 'Help', browser: 'help', editor: 'help' },
] as const;
export type Destination = typeof MAIN_DESTINATIONS[number]['id'];
export function browserDestination(page: string): Destination | null {
  if (page === 'usage' || page === 'logs') return 'activity';
  if (page === 'storage' || page === 'recovery') return 'service';
  return MAIN_DESTINATIONS.find(entry => entry.browser === page)?.id ?? null;
}
export function editorDestination(route: string): Destination | null {
  if (route.startsWith('search/')) return 'search';
  if (route.startsWith('indexing/') || route === 'manage/overview') return 'indexes';
  if (route === 'manage/usage' || route === 'manage/logs') return 'activity';
  if (route === 'manage/storage' || route === 'manage/repair') return 'service';
  return MAIN_DESTINATIONS.find(entry => entry.editor === route)?.id ?? null;
}
