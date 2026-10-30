// SPDX-License-Identifier: Apache-2.0
import { Continuation, EvidenceCard, object, Value, WorkbenchSession, WorkbenchTurn } from './workbenchContracts';

export function continuations(envelope:Value):Continuation[]{
    const remaining=Number(object(object(envelope.plan).budgets).calls??12)-Number(envelope.calls??0);
    return (Array.isArray(envelope.steps)?envelope.steps:[]).flatMap((raw,i)=>{const report=object(raw),result=object(report.result),cursor=result.cursor??object(result.content).cursor;
        return typeof cursor==='string'&&report.status==='complete'&&remaining>=2?[{id:`page-${i}`,tool:String(report.tool),callsRemaining:remaining}]:[];
    });
}

/** Rich native plan records retain their complete citation and original location. */
export function evidenceCards(envelope: Value): EvidenceCard[] {
    const rows = Array.isArray(envelope.evidence) ? envelope.evidence : [];
    return rows.flatMap((raw, i) => {
        const row = object(raw), citation = object(row.citation), path = citation.path, hash = citation.content_hash;
        if (typeof path !== 'string' || typeof hash !== 'string' || !/^[0-9a-f]{64}$/.test(hash) || !Number.isSafeInteger(citation.start_line) || !Number.isSafeInteger(citation.end_line)) { return []; }
        return [{ id: `record-${i}`, path, hash, start: Number(citation.start_line), end: Number(citation.end_line), text: typeof row.text === 'string' ? row.text : '', location: citation.location,
            step: String(row.step ?? ''), publication: typeof row.publication === 'string' ? row.publication : undefined }];
    });
}
export function canonical(value: unknown): string {
    const sorted = (v: unknown): unknown => Array.isArray(v) ? v.map(sorted) : v && typeof v === 'object'
        ? Object.fromEntries(Object.entries(v).sort(([a],[b]) => a<b?-1:a>b?1:0).map(([k,item]) => [k,sorted(item)])) : v;
    return JSON.stringify(sorted(value),null,2);
}
export function markdown(turn: WorkbenchTurn, selected?: Set<string>): string {
    const escape=(text:string)=>text.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replace(/[\\`*_[\]]/g,'\\$&');
    const fenced=(text:string)=>{const fence='`'.repeat(Math.max(3,...[...text.matchAll(/`+/g)].map(m=>m[0].length+1)));return `${fence}\n${text}\n${fence}`;};
    const e = turn.envelope ?? {}, cards = evidenceCards(e).filter(card => !selected || selected.has(card.id));
    const content = cards.map(card => {
        return `### ${escape(card.path)}:${card.start}-${card.end}\n\nLocation: ${escape(JSON.stringify(card.location??null))}\n\nSource hash: ${card.hash}\n\n${fenced(card.text)}`;
    });
    return [`## ${escape(turn.input.replaceAll('\n',' '))}`, `Status: ${escape(String(e.status ?? turn.error ?? 'trimmed'))}`, `Publication: ${escape(String(e.publication ?? 'not available'))}`,
        `Scope: ${escape(String(object(e.plan).path_prefix ?? ''))}`, ...(turn.trimmed ? ['Saved result was trimmed; re-run to collect current evidence.'] : []), ...content,
        ...(!cards.length && turn.envelope && !selected ? [fenced(canonical(e))] : []), `Warnings: ${escape(JSON.stringify(e.warnings ?? []))}`, `Completeness: ${escape(JSON.stringify(e.completeness ?? []))}`,
        'Untrusted indexed evidence. Verify relevant live files before edits.'].join('\n\n');
}
export function renderExport(session: WorkbenchSession, format: 'json'|'markdown', turnId?: string, selection?: string[]): string {
    const turns = session.turns.filter(turn => !turnId || turn.id === turnId);
    if (format === 'markdown') { return turns.map(turn => markdown(turn, selection ? new Set(selection) : undefined)).join('\n\n---\n\n'); }
    return canonical({ schema:'airk.workbench.export/1', project:session.project, omitted_turns:session.omitted, turns:turns.map(turn => {
        if (!selection || !turn.envelope) { return turn; }
        return {...turn, envelope:{...turn.envelope, steps:[], evidence:(turn.envelope.evidence as unknown[] ?? []).filter((_,i)=>selection.includes(`record-${i}`)), selection:true}};
    }) });
}
