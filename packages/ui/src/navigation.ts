// SPDX-License-Identifier: Apache-2.0
// Presentation destinations map to retained host routes; they grant no authority.
export const MAIN_DESTINATIONS = [
  { id: 'overview', title: 'Overview', browser: 'overview', editor: 'overview' },
  { id: 'search', title: 'Search', browser: 'search', editor: 'search/query' },
  { id: 'indexes', title: 'Indexes', browser: 'index-library', editor: 'indexing/library' },
  { id: 'activity', title: 'Activity', browser: 'activity', editor: 'manage/activity' },
  { id: 'agents', title: 'Agents', browser: 'agents', editor: 'manage/agents' },
  { id: 'service', title: 'Service', browser: 'service', editor: 'manage/service' },
  { id: 'code', title: 'Code', browser: 'search', editor: 'code/symbols' },
  { id: 'docs', title: 'Documents', browser: 'search', editor: 'docs/facts' },
  { id: 'evidence', title: 'Evidence', browser: 'search', editor: 'evidence/inspector' },
  { id: 'review', title: 'Review', browser: 'search', editor: 'review/changes' },
  { id: 'strategies', title: 'Strategies', browser: 'search', editor: 'strategies/library' },
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
  const group = MAIN_DESTINATIONS.find(entry => entry.editor.split('/')[0] === route.split('/')[0] && isEvidenceToolRoute(route));
  if (group) return group.id;
  if (route.startsWith('indexing/') || route === 'manage/overview') return 'indexes';
  if (route === 'manage/usage' || route === 'manage/logs') return 'activity';
  if (route === 'manage/storage' || route === 'manage/repair') return 'service';
  return MAIN_DESTINATIONS.find(entry => entry.editor === route)?.id ?? null;
}


/** Guided workflow entries, not a native tool catalog or authority source. */
export const EVIDENCE_TOOLS = [
  {title:'Code',guide:'explorer.code.symbols',description:'Find symbols, textual occurrences and selected-source outlines. Written references are not resolved cross-file bindings.',entries:[['code/symbols','Symbols'],['code/references','Textual references'],['code/outline','Selected-source outline']]},
  {title:'Documents',guide:'explorer.docs.facts',description:'Inspect indexed document facts with extraction limits and original page, slide or sheet locations.',entries:[['docs/facts','Document facts']]},
  {title:'Evidence',guide:'explorer.evidence.inspector',description:'Inspect citations, check their saved-source state and compare indexed publications. Indexed changes are distinct from Git changes.',entries:[['evidence/inspector','Inspector'],['evidence/checker','Check citations'],['evidence/changes','Indexed changes']]},
  {title:'Review',guide:'explorer.review.changes',description:'Review Git staged/worktree context, bounded impact and test candidates. These views do not execute tests or establish complete semantic impact.',entries:[['review/changes','Git changes'],['review/impact','Impact candidates'],['review/tests','Test candidates']]},
  {title:'Strategies',guide:'strategies.library',description:'Keep guided Library, Studio and Planner workflows, source checks, complete exports and explicit save/import/overwrite review. No autonomous orchestration.',entries:[['strategies/library','Library'],['strategies/studio','Studio'],['strategies/planner','Planner']]},
] as const;
export function isEvidenceToolRoute(route:unknown):boolean {
  return typeof route==='string'&&EVIDENCE_TOOLS.some(group=>group.entries.some(([entry])=>entry===route));
}
export function evidenceToolLabel(route:unknown):string|undefined {
  for(const group of EVIDENCE_TOOLS){const entry=group.entries.find(([key])=>key===route);if(entry)return `${group.title}: ${entry[1]}`;}
  return undefined;
}
export type EvidenceToolsNode={tag:'section'|'h2'|'h3'|'p'|'div'|'button'|'a';children:readonly(EvidenceToolsNode|string)[];route?:string;href?:string};
/** Shared named hub. Host actions remain existing guided routes; no read is issued here. */
export function evidenceToolsPresentation(host:'editor'|'browser',lastRoute?:string,browserRoutes:readonly string[]=[]):EvidenceToolsNode {
  const node=(tag:EvidenceToolsNode['tag'],...children:(EvidenceToolsNode|string)[]):EvidenceToolsNode=>({tag,children});
  return node('section',node('h2','Evidence tools'),
    node('p','Choose a guided workflow. Existing inputs, complete native results, citations, publication identity and full exports stay with their owning view.'),
    ...(host==='editor'&&isEvidenceToolRoute(lastRoute)?[{...node('button',`Continue ${evidenceToolLabel(lastRoute)}`),route:lastRoute}]:[]),
    ...(host==='browser'&&browserRoutes.length===0?[node('p','These guided workflows currently open in AIRK Explorer in VS Code. Browser workflow migration is not complete. Open AIRK Explorer → Search → Evidence tools for the named view; browser and editor scopes are separate. The browser native read form remains available without being represented as a replacement for these workflows.')]:[]),
    ...EVIDENCE_TOOLS.map(group=>node('section',node('h3',group.title),node('p',group.description),
      ...(host==='editor'?group.entries.map(([route,label])=>({...node('button',`${group.title}: ${label}`),route}))
        :[...group.entries.filter(([route])=>browserRoutes.includes(route)).map(([route,label])=>({...node('button',`${group.title}: ${label}`),route})),...(group.entries.some(([route])=>!browserRoutes.includes(route))?[node('p',`Editor workflows: ${group.entries.filter(([route])=>!browserRoutes.includes(route)).map(([,label])=>label).join(', ')}. Their browser migration remains incomplete; scopes and host actions are separate.`)]:[]),{...node('a',`Read ${group.title} guide`),href:`#help?guide=${encodeURIComponent(group.guide)}`}]))));
}

/** Original destination-keyed vectors, shared by both rails. */
export const DESTINATION_ICONS: Readonly<Record<Destination, string>> = Object.freeze({
  overview: 'M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z',
  search: 'M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15M16 16l5 5',
  indexes: 'M3 5h18v5H3zM3 14h18v5H3zM6 7.5h.01M6 16.5h.01',
  activity: 'M3 12h4l3-8 4 16 3-8h4',
  agents: 'M5 8h14v12H5zM12 3v5M9 12h.01M15 12h.01M9 16h6M2 12h3M19 12h3',
  service: 'M4 7h16M4 17h16M8 4v6M16 14v6',
  code: 'm8 6-6 6 6 6M16 6l6 6-6 6M14 3l-4 18',
  docs: 'M5 3h9l5 5v13H5zM14 3v6h5M8 13h8M8 17h6',
  evidence: 'M12 2 3 6v6c0 5 9 10 9 10s9-5 9-10V6zM8 12l3 3 5-6',
  review: 'M9 5h12M9 12h12M9 19h12M2 5l1 1 2-2M2 12l1 1 2-2M2 19l1 1 2-2',
  strategies: 'M4 4h6v6H4zM14 14h6v6h-6zM14 7h5v7M7 10v7h7',
  help: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M9 9a3 3 0 1 1 4 3c-1 .5-1 1-1 2M12 17h.01',
});
