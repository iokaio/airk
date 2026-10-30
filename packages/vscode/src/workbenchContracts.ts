// SPDX-License-Identifier: Apache-2.0
/** Pure host/webview contract. Evidence content is always inert text. */
export type Value = Record<string, unknown>;
export const WORKBENCH_HELP_ROUTE='help?focus=engine-workbench';
export interface WorkbenchTurn { id: string; at: number; input: string; format: 'structured' | 'json' | 'markdown'; request?: Value; envelope?: Value; error?: string; trimmed?: boolean }
export interface WorkbenchSession { schema: 1; id: string; project: string; title: string; createdAt: number; updatedAt: number; revision: number; omitted: number; turns: WorkbenchTurn[] }
export interface EvidenceCard { id: string; path: string; start: number; end: number; hash: string; text: string; location?: unknown; step: string; publication?: string }
export interface Continuation { id:string; tool:string; callsRemaining:number }
export interface WorkbenchState {
    projects: { id: string; name: string }[]; project?: string; session?: WorkbenchSession;
    recent: { id: string; title: string; updatedAt: number; turns: number }[];
    cards: Record<string, EvidenceCard[]>; busy: boolean; notice?: string; save: string;
    continuations?:Record<string,Continuation[]>;
    status?: Value; persistence: boolean; operations?: Value;
}
export type WorkbenchMessage =
    | { type: 'ready' | 'stop' | 'new' | 'clear' | 'manage' | 'help' | 'refresh' | 'retrySave' | 'fork' | 'forms' }
    | { type: 'project' | 'recent' | 'delete'; id: string }
    | { type: 'rename'; title: string }
    | { type: 'run'; input: string; prepare: boolean; recipe?: string; plan?: Value; pathPrefix?: string }
    | { type: 'rerun' | 'saveStrategy' | 'remember'; turn: string }
    | { type: 'open' | 'context'; turn: string; card: string }
    | { type:'continue'; turn:string; step:string }
    | { type: 'copy' | 'export'; turn?: string; format: 'json' | 'markdown'; cards?: string[] };
export const object = (v: unknown): Value => v && typeof v === 'object' && !Array.isArray(v) ? v as Value : {};
export function parseWorkbenchMessage(raw: unknown): WorkbenchMessage | undefined {
    const v = object(raw), type = v.type;
    const str = (k: string, max = 128) => typeof v[k] === 'string' && (v[k] as string).length <= max && !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(v[k] as string);
    const exact = (keys: string[]) => Object.keys(v).every(k => keys.includes(k));
    if (typeof type !== 'string' || JSON.stringify(raw).length > 40000) { return; }
    if (['ready','stop','new','clear','manage','help','refresh','retrySave','fork','forms'].includes(type) && exact(['type'])) { return v as WorkbenchMessage; }
    if (['project','recent','delete'].includes(type) && exact(['type','id']) && str('id')) { return v as WorkbenchMessage; }
    if (type === 'rename' && exact(['type','title']) && str('title',80)) { return v as WorkbenchMessage; }
    if (type === 'run' && exact(['type','input','prepare','recipe','plan','pathPrefix']) && str('input',4096) && typeof v.prepare === 'boolean' && (v.pathPrefix===undefined || str('pathPrefix',1024))
        && (v.recipe === undefined || str('recipe')) && (v.plan === undefined || Object.keys(object(v.plan)).length > 0)) { return v as WorkbenchMessage; }
    if (['rerun','saveStrategy','remember'].includes(type) && exact(['type','turn']) && str('turn')) { return v as WorkbenchMessage; }
    if (['open','context'].includes(type) && exact(['type','turn','card']) && str('turn') && str('card')) { return v as WorkbenchMessage; }
    if(type==='continue'&&exact(['type','turn','step'])&&str('turn')&&str('step')){return v as WorkbenchMessage;}
    if (['copy','export'].includes(type) && exact(['type','turn','format','cards']) && ['json','markdown'].includes(String(v.format)) && (v.turn === undefined || str('turn'))
        && (v.cards === undefined || Array.isArray(v.cards) && v.cards.length <= 1000 && v.cards.every(id => typeof id === 'string' && id.length <= 128))) { return v as WorkbenchMessage; }
}
