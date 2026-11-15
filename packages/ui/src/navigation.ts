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
  if (route.startsWith('search/') || isEvidenceToolRoute(route)) return 'search';
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
