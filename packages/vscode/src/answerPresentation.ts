// SPDX-License-Identifier: Apache-2.0
// The one Answer /3 presentation for people: Workbench, Admin and their copies
// and exports all render this tree. Types are the generated Core views; the
// record projection (`airk.arbiter.loop-evidence/1`) has no generated view
// yet, so its fields are read defensively and every record is also shown whole.
import type { AnswerPart, AnswerTicket, ContextResponseV3, Coverage, DecisionRecord, DecisionSource, Degradation, LoopEvidence,
    PartialMarker, PartStop, RecordUse, SubNeed, UnrunRead } from './generated/arbiter';

export type AnswerNode = { tag: 'section' | 'h3' | 'h4' | 'h5' | 'p' | 'div' | 'button' | 'span' | 'ul' | 'li' | 'details' | 'summary' | 'article' | 'pre';
    children: readonly (AnswerNode | string)[]; id?: string; citationTarget?: string;
    /** A cited span the host may open: record id and span position in its projection. */
    sourceLocator?: { record: string; span: number } };
type Child = AnswerNode | string;
const node = (tag: AnswerNode['tag'], ...children: Child[]): AnswerNode => ({ tag, children });
const words = (value: string): string => value.replaceAll('_', ' ');
const json = (value: unknown): string => JSON.stringify(value, null, 2);
type Rec = Record<string, unknown>;
const rec = (value: unknown): Rec => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Rec : {};
const short = (id: string): string => id.slice(0, 12);

/** The question's text for a part's UTF-8 byte span; a span off a character boundary shows its bytes. */
export function questionSpanText(question: string, start: number, end: number): string {
    const bytes = new TextEncoder().encode(question);
    if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || end > bytes.length || start > end) return `[bytes ${start}–${end} outside the question]`;
    return new TextDecoder().decode(bytes.slice(start, end)).trim();
}

/** One span of a record's compact projection, as returned. */
export interface ProjectionSpan { position: number; raw: Rec; path?: string; start?: number; end?: number; hash?: string; text?: string }
export function projectionSpans(record: Pick<LoopEvidence, 'canonical_result'>): ProjectionSpan[] {
    const spans = rec(record.canonical_result).spans;
    return Array.isArray(spans) ? spans.map((raw, position) => {
        const span = rec(raw);
        return { position, raw: span, ...(typeof span.path === 'string' ? { path: span.path } : {}),
            ...(Number.isSafeInteger(span.start_line) ? { start: Number(span.start_line) } : {}), ...(Number.isSafeInteger(span.end_line) ? { end: Number(span.end_line) } : {}),
            ...(typeof span.content_hash === 'string' ? { hash: span.content_hash } : {}), ...(typeof span.text === 'string' ? { text: span.text } : {}) };
    }) : [];
}

export function decisionSourceLabel(source: DecisionSource): string {
    switch (source.kind) {
        case 'model': return `Model decided (${words(source.mode)}${source.rule == null ? '' : `, rule ${source.rule}`})`;
        case 'deterministic': return `Deterministic order (${words(source.rule)}); no model call`;
        case 'abstained': return `Model abstained (${words(source.reason)}); deterministic fallback ${words(source.fallback)}`;
        case 'unavailable': return `Model call sent but refused or lost (${words(source.reason)}); deterministic fallback ${words(source.fallback)}`;
        case 'not_dispatched': return `Model present but not called (${words(source.reason)}); deterministic fallback ${words(source.fallback)}`;
        default: { const unknown: never = source; return `Decision source not known to this client: ${JSON.stringify(unknown)}`; }
    }
}
export function degradationLabel(value: Degradation): string {
    switch (value.kind) {
        case 'context_absent': return 'No Context model call was made: every decision used the deterministic order or was not dispatched.';
        case 'context_abstained': return `The Context model abstained ${value.count} time(s); the deterministic order decided those.`;
        case 'focus_absent': return 'Focus ranking did not run.';
        case 'model_share': return `Part ${value.part} ran out of its model share at the ${words(value.stage)} stage.`;
        case 'model_outcome_unknown': return 'Some model work has an unknown outcome; it stays charged.';
        case 'configuration_changed': return 'The configuration changed during this Answer.';
        case 'batch_split': return 'A batched model call was split.';
        case 'search_focus_deferred': return 'Focus inside searches was deferred to the batched Focus calls.';
        case 'parts': case 'sub_needs': case 'rounds': return `${words(value.kind)} reduced from ${value.requested} to ${value.applied} (${words(value.reason)}).`;
        case 'model': return `The ${value.stage} model ${words(value.requested)} was replaced by ${value.applied == null ? 'none' : words(value.applied)} (${words(value.reason)}).`;
        case 'device': return `The ${value.stage} stage ran on ${words(value.applied)} instead of ${words(value.requested)} (${words(value.reason)}).`;
        default: { const unknown: never = value; return `Reduction not known to this client: ${JSON.stringify(unknown)}`; }
    }
}
const stopLabel: Record<PartStop, string> = {
    covered: 'every sub-need covered', clarification_required: 'the question needs clarification', no_justified_read: 'no further read was justified',
    no_progress: 'a round found nothing new', rounds: 'the round limit was reached', budget: 'the budget ran out', deadline: 'the deadline was reached',
    stale: 'the index publication changed', refused: 'a read was refused', unsupported: 'the request is not supported',
};
const coverageLabel: Record<Coverage, string> = { covered: 'Covered', partial: 'Partly covered', missing: 'Missing' };

/** What state the reply is in, for status lines and for a later job client. */
export interface AnswerState { kind: 'complete' | 'partial' | 'queued'; partial: PartialMarker | null; ticket: AnswerTicket | null; continuation?: string }
export function answerState(answer: Pick<ContextResponseV3, 'partial' | 'ticket'>): AnswerState {
    const continuation = answer.partial?.continuation ?? answer.ticket?.id ?? undefined;
    return { kind: answer.ticket ? 'queued' : answer.partial ? 'partial' : 'complete', partial: answer.partial, ticket: answer.ticket, ...(continuation ? { continuation } : {}) };
}
export function answerStatusLine(answer: Pick<ContextResponseV3, 'partial' | 'ticket'>): string {
    const state = answerState(answer), partial = state.partial;
    if (state.kind === 'queued') return `Queued: position ${state.ticket!.queue_position}, estimated start in ${state.ticket!.estimated_start_ms} ms, finish in ${state.ticket!.estimated_finish_ms} ms.`;
    if (partial) return `Partial: stopped at the ${words(partial.stage)} stage (${words(partial.reason)})${partial.part == null ? '' : ` in part ${partial.part}`}.`;
    return 'Complete within the request budget.';
}

/** Everything a reply says about one Answer partition, grouped by part then sub-need. */
export function answerPresentation(answer: ContextResponseV3, identity: string): AnswerNode {
    const retained = new Map(answer.records.map(record => [record.id, record]));
    const omitted = new Set(answer.omitted_evidence);
    const target = (id: string) => `answer-${identity}-${id}`;
    const usesOf = (id: string): RecordUse[] => answer.record_uses.filter(use => use.record === id);
    const useLabel = (use: RecordUse) => `part ${use.part}${use.need == null ? '' : ` sub-need ${use.need}`}, round ${use.round}, ${words(use.cause)}`;
    // Each text is carried once (Core "Where the text is"): a row names the row
    // holding its text by {record, pointer}; the carrier lists other places.
    const rows = answer.records.flatMap(record => projectionSpans(record).map(span => ({ record, span })));
    const byPointer = new Map(rows.filter(row => typeof row.span.raw.pointer === 'string').map(row => [`${row.record.id}|${String(row.span.raw.pointer)}`, row]));
    const place = (span: ProjectionSpan) => span.path ? `${span.path}${span.start ? `:${span.start}${span.end && span.end !== span.start ? `–${span.end}` : ''}` : ''}` : `span ${span.position} without a location`;
    const byPlace = new Map<string, { record: LoopEvidence; span: ProjectionSpan }>();
    for (const row of rows) if (row.span.path && row.span.start !== undefined) byPlace.set(`${row.span.path}:${row.span.start}-${row.span.end ?? row.span.start}`, row);
    const rowTarget = (record: string, position: number) => `answer-${identity}-${record}-span-${position}`;
    const named = (reference: unknown): { record: LoopEvidence; span: ProjectionSpan } | undefined => {
        const value = rec(reference);
        return typeof value.record === 'string' && typeof value.pointer === 'string' ? byPointer.get(`${value.record}|${value.pointer}`) : undefined;
    };
    const jump = (row: { record: LoopEvidence; span: ProjectionSpan }, label: string): AnswerNode => ({ ...node('button', label), citationTarget: rowTarget(row.record.id, row.span.position) });
    const open = (record: LoopEvidence, span: ProjectionSpan, label: string): AnswerNode => span.path && span.hash
        ? { ...node('button', label), sourceLocator: { record: record.id, span: span.position } } : node('span', label);
    /** One returned row; `anchor` marks the copy in the record list that citations jump to. */
    const spanLine = (record: LoopEvidence, span: ProjectionSpan, anchor = false): AnswerNode => {
        const raw = span.raw, notes: Child[] = [], text = (value: string) => notes.push(node('span', ` · ${value}`));
        if (typeof raw.focus === 'number') text(`Focus ${raw.focus.toFixed(3)}`);
        if (raw.truncated === true) text('excerpt cut');
        if (Object.keys(rec(raw.same_as)).length) {
            const carrier = named(raw.same_as);
            notes.push(node('span', ` · ${raw.within === true ? 'its text lies within' : 'same text as'} `),
                carrier ? jump(carrier, `${place(carrier.span)} (record ${short(carrier.record.id)})`) : node('span', `${JSON.stringify(raw.same_as)}, which this reply does not carry`));
        } else if (raw.within === true) text('lies within another span');
        if (Array.isArray(raw.also_at) && raw.also_at.length) {
            notes.push(node('span', ' · also at '));
            raw.also_at.forEach((value, index) => {
                const label = typeof value === 'string' ? value : JSON.stringify(value), other = typeof value === 'string' ? byPlace.get(value) : undefined;
                notes.push(...(index ? [node('span', ', ')] : []), other ? open(other.record, other.span, `Open ${label}`) : node('span', label));
            });
        }
        if (Array.isArray(raw.overlaps)) for (const overlap of raw.overlaps) {
            const value = rec(overlap), other = named(overlap);
            notes.push(node('span', ` · ${String(value.bytes ?? '?')} bytes at its ${String(value.side ?? '?')} are shown in `),
                other ? jump(other, `${place(other.span)} (record ${short(other.record.id)})`) : node('span', JSON.stringify(overlap)));
        }
        const item = node('li', open(record, span, `Open ${place(span)}`), ...notes, ...(span.text !== undefined ? [node('pre', span.text)] : []));
        return anchor ? { ...item, id: rowTarget(record.id, span.position) } : item;
    };
    const citation = (id: string): AnswerNode => {
        const record = retained.get(id);
        if (!record) return node('li', omitted.has(id) ? `Evidence ${short(id)} was cited but omitted from this reply to fit its size; its id is ${id}.`
            : `Evidence ${id} is cited but neither returned nor listed as omitted.`);
        const spans = projectionSpans(record);
        return node('li', { ...node('button', `${words(record.operation)} ${short(id)} · ${words(record.role)}`), citationTarget: target(id) },
            node('span', ` · ${JSON.stringify(record.arguments)}`),
            ...(spans.length ? [node('ul', ...spans.map(span => spanLine(record, span)))] : [node('span', ' · no source span in this record; see the record below.')]));
    };
    const unrun = (read: UnrunRead) => node('li', `${words(read.operation)} ${read.target} — not run (${words(read.reason)}). You can run it yourself.`);
    const decisionLine = (decision: DecisionRecord): AnswerNode => {
        const scores = decision.top == null ? '' : ` · top ${decision.top}${decision.runner_up == null ? '' : `, runner-up ${decision.runner_up}`}${decision.margin == null ? '' : `, margin ${decision.margin}`}`;
        const model = decision.model == null ? '' : ` · ${words(decision.model)}${decision.device ? ` on ${words(decision.device)}` : ''}${decision.precision ? ` ${decision.precision}` : ''}`;
        return node('li', `#${decision.ordinal} ${words(decision.stage)}${decision.part == null ? '' : ` part ${decision.part}`}${decision.need == null ? '' : ` sub-need ${decision.need}`}${decision.round == null ? '' : ` round ${decision.round}`}: `
            + `chose ${decision.chosen.map(card => `${words(card.class)} (${card.symbol})`).join(', ') || 'nothing'}. ${decisionSourceLabel(decision.source)}${model}${scores}`
            + `${decision.applicability === 'current' ? '' : ` · ${words(decision.applicability)}`}${decision.work_unknown ? ' · work unknown' : ''}.`);
    };
    const subNeed = (part: AnswerPart, need: SubNeed): AnswerNode => {
        const decisions = answer.decisions.filter(decision => decision.part === part.id && decision.need === need.id);
        return node('section', node('h5', `Sub-need ${part.id}.${need.id} (${words(need.kind)}): ${need.text}`),
            node('p', `${coverageLabel[need.coverage]}. ${need.evidence.length} cited record(s)${need.evidence_omitted ? `, ${need.evidence_omitted} omitted from this reply` : ''}.`),
            ...(need.evidence.length ? [node('ul', ...need.evidence.map(citation))] : []),
            ...(need.unrun_reads.length ? [node('p', 'Follow-ups chosen or proposed but not run:'), node('ul', ...need.unrun_reads.map(unrun))] : []),
            ...(decisions.length ? [node('details', node('summary', `How it was decided: ${[...new Set(decisions.map(decision => decision.source.kind === 'model' ? 'model' : words(decision.source.kind)))].join(', ')}`),
                node('ul', ...decisions.map(decisionLine)))] : []));
    };
    const partSection = (part: AnswerPart): AnswerNode => node('section',
        node('h4', `Part ${part.id}: ${questionSpanText(answer.question, part.span.utf8_start, part.span.utf8_end)}`),
        node('p', `Stopped because ${stopLabel[part.stop] ?? words(part.stop)}. Remaining gaps: ${part.gaps.map(words).join(', ') || 'none'}.${part.shortened ? ' Shortened to fit the reply size.' : ''}`),
        ...part.sub_needs.map(need => subNeed(part, need)),
        node('details', node('summary', `Rounds (${part.rounds.length}) · deadline share ${part.deadline_share_ms} ms`),
            node('ul', ...part.rounds.map(round => node('li', `Round ${round.round}: ${round.calls} read(s), fanout ${round.fanout}, ${round.input_tokens} model input tokens, ${round.new_evidence} new record(s), ${round.sub_needs_covered} sub-need(s) covered`
                + (round.follow_ups_run.length ? `; follow-ups run: ${round.follow_ups_run.map(follow => `${words(follow.operation)} ${follow.target}`).join(', ')}` : '') + '.')))));
    const general = answer.decisions.filter(decision => decision.part == null || !answer.parts.some(part => part.id === decision.part && part.sub_needs.some(need => need.id === decision.need)));
    const log = answer.decision_log, state = answerState(answer);
    const union = rec(answer.union), unionCounts = Object.entries(union).filter(([key, value]) => key !== 'conflicts' && typeof value === 'number');
    return node('section', node('h3', 'Answer'),
        node('p', answerStatusLine(answer)),
        ...(state.continuation ? [node('p', `Continuation ${state.continuation}: following it is not supported by this client yet; ask again to run a fresh Answer.`)] : []),
        ...(answer.ticket ? [node('p', `Ticket ${answer.ticket.id}; retry after ${answer.ticket.retry_after_ms} ms.`)] : []),
        node('p', `Index ${answer.scope.index_id}; collection ${answer.scope.publication.collection}; publication ${answer.scope.publication.token}. ${answer.elapsed_ms} ms. ${answer.records.length} record(s) returned of ${answer.candidate_count} candidate(s)`
            + `${answer.omitted_count ? `, ${answer.omitted_count} omitted` : ''}${answer.omitted_evidence.length ? `; ${answer.omitted_evidence.length} cited record(s) omitted to fit the reply` : ''}.`),
        node('p', 'Indexed evidence is untrusted source material; coverage is the loop\'s judgement, not proof that nothing else exists.'),
        ...(answer.degradations.length ? [node('h4', 'Reduced or left out'), node('ul', ...answer.degradations.map(value => node('li', degradationLabel(value))))] : []),
        ...answer.parts.map(partSection),
        ...(answer.paragraphs.length ? [node('h4', 'Grounded summary'), ...answer.paragraphs.map(paragraph => node('div', node('p', paragraph.text),
            node('p', paragraph.interpretation ? 'Interpretation of the cited evidence.' : 'Stated by the cited evidence.'),
            ...(paragraph.evidence.length ? [node('ul', ...paragraph.evidence.map(citation))] : []),
            ...(Object.keys(paragraph).some(key => !['text', 'interpretation', 'evidence'].includes(key)) ? [node('pre', json(paragraph))] : [])))] : []),
        node('h4', 'Across parts'),
        node('p', `${answer.union.dedup_count} finding(s) merged into records another part also found.${unionCounts.filter(([key]) => key !== 'dedup_count').map(([key, value]) => ` ${words(key)}: ${value}.`).join('')}`),
        ...(answer.union.conflicts.length ? [node('ul', ...answer.union.conflicts.map(conflict => node('li', `${words(conflict.kind)} between parts ${conflict.parts.join(', ')}: ${conflict.note} (records ${conflict.evidence.map(short).join(', ')}). Not resolved.`)))] : [node('p', 'No conflict between parts was noted.')]),
        node('h4', 'Decisions'),
        node('p', `${log.total} decision(s) in the service's log; ${log.included} shown here, ${log.omitted} omitted from this reply (oldest deterministic first). ${log.card_calls} model card call(s), ${log.focus_calls} Focus call(s) over ${log.focus_pairs} pair(s)${log.work_unknown ? `, ${log.work_unknown} with unknown work` : ''}.`),
        node('ul', ...log.counts.map(count => node('li', `${words(count.stage)}: ${count.count} ${words(count.source)}${count.model ? ` (${words(count.model)})` : ''}`))),
        ...(general.length ? [node('details', node('summary', 'Split, plan and other decisions'), node('ul', ...general.map(decisionLine)))] : []),
        node('details', node('summary', `Every included decision (${answer.decisions.length})`), node('ul', ...answer.decisions.map(decisionLine))),
        node('h4', 'Evidence records'),
        ...answer.records.map(record => {
            const projection = rec(record.canonical_result), spans = projectionSpans(record), rows = Array.isArray(projection.rows) ? projection.rows.map(rec) : [];
            const counts = [typeof projection.hits_total === 'number' ? `${spans.length} of ${projection.hits_total} span(s) shown` : `${spans.length} span(s)`,
                ...(typeof projection.rows_total === 'number' ? [`${rows.length} of ${projection.rows_total} location row(s)`] : []),
                ...(typeof projection.full_bytes === 'number' ? [`full read ${projection.full_bytes} bytes kept by the service`] : [])];
            return node('details', node('summary', `${words(record.operation)} · ${words(record.role)} · ${short(record.id)}`),
                { ...node('article', node('h5', record.id), node('p', `Arguments: ${JSON.stringify(record.arguments)}. ${counts.join('; ')}.`),
                    node('p', `Found by: ${usesOf(record.id).map(useLabel).join('; ') || 'no recorded use'}.`),
                    ...(typeof projection.failure === 'string' ? [node('p', `The read failed: ${projection.failure}`)] : []),
                    ...(spans.length ? [node('ul', ...spans.map(span => spanLine(record, span, true)))] : []),
                    ...(rows.length ? [node('ul', ...rows.map(row => node('li', `${String(row.path ?? '')}:${String(row.start_line ?? '')}${row.end_line != null && row.end_line !== row.start_line ? `–${String(row.end_line)}` : ''} ${String(row.text ?? '')}`)))] : []),
                    node('details', node('summary', 'Complete returned record'), node('pre', json(record)))), id: target(record.id) });
        }),
        node('details', node('summary', 'Work, model use, timing and applications'),
            node('pre', json({ elapsed_ms: answer.elapsed_ms, work: answer.work, model_work: answer.model_work, decision_log: answer.decision_log, applications: answer.applications, union: answer.union, partial: answer.partial, ticket: answer.ticket }))));
}

/** Markdown for copies and exports of the same tree; nothing the tree shows is dropped. */
export function answerNodeMarkdown(root: AnswerNode): string {
    const escape = (text: string) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replace(/[\\`*_[\]]/g, '\\$&');
    const fence = (text: string) => { const mark = '`'.repeat(Math.max(3, ...[...text.matchAll(/`+/g)].map(match => match[0].length + 1))); return `${mark}\n${text}\n${mark}`; };
    const inline = (value: Child): string => typeof value === 'string' ? escape(value) : value.tag === 'pre' ? '' : value.children.map(inline).join('');
    const blocks = (value: Child, depth = 0): string[] => {
        if (typeof value === 'string') return [escape(value)];
        const own = value.children;
        switch (value.tag) {
            case 'h3': return [`### ${own.map(inline).join('')}`];
            case 'h4': return [`#### ${own.map(inline).join('')}`];
            case 'h5': return [`##### ${own.map(inline).join('')}`];
            case 'pre': return [fence(own.map(child => typeof child === 'string' ? child : '').join(''))];
            case 'summary': return [`**${own.map(inline).join('')}**`];
            case 'ul': return [own.map(child => {
                const [head, ...rest] = blocks(child, depth + 1);
                return [`${'  '.repeat(depth)}- ${head ?? ''}`, ...rest.map(line => line.split('\n').map(text => `${'  '.repeat(depth + 1)}${text}`).join('\n'))].join('\n');
            }).join('\n')];
            case 'li': {
                const text = own.filter(child => typeof child === 'string' || !['ul', 'pre'].includes(child.tag)).map(inline).join('');
                return [text, ...own.flatMap(child => typeof child !== 'string' && ['ul', 'pre'].includes(child.tag) ? blocks(child, depth) : [])];
            }
            case 'p': case 'span': case 'button': return [own.map(inline).join('')];
            default: return own.flatMap(child => blocks(child, depth));
        }
    };
    return blocks(root).filter(text => text.length).join('\n\n');
}
