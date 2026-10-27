// SPDX-License-Identifier: Apache-2.0
// Runs a validated strategy against one pinned publication. Every call goes through the
// Explorer's evidence session; nothing here writes files, runs commands or calls an agent.
import { createHash } from 'node:crypto';
import type { LiveCheck } from './explorerContracts';
import { explorerError } from './explorerErrors';
import { EvidenceClient, EvidenceFailure, NativeTool, ToolResult } from './explorerEvidence';
import { FactHandles, factList } from './explorerFacts';
import { relativePrefix, searchView } from './explorerSearch';
import {
    bind, clipRecords, LedgerRecord, operator, orderRecords, RUN_SCHEMA, RunResult, StepReport, Strategy, StrategyStep, unavailableReason,
} from './explorerStrategies';

type Value = Record<string, unknown>;
interface Location { readonly path: string; readonly startLine: number; readonly endLine: number; readonly contentHash?: string }
interface StepOutput { readonly locations: Location[]; readonly symbols: string[] }

export interface RunRequest {
    readonly strategy: Strategy;
    readonly params: Readonly<Record<string, string>>;
    readonly scope: Readonly<Record<string, string>>;
}
export interface RunDependencies {
    readonly session: EvidenceClient;
    readonly facts: FactHandles;
    readonly project: string;
    readonly extensionVersion: string;
    /** Saved-bytes check for one indexed file, given AIRK's own source location. */
    live(root: string, relative: string, contentHash: string): Promise<LiveCheck>;
    readonly now?: () => number;
}

class Stop extends Error {}

/** Check required parameters and path scope before anything runs. */
export function checkRequest(request: RunRequest): void {
    const reason = unavailableReason(request.strategy);
    if (reason) { throw new EvidenceFailure(explorerError('CAPABILITY_DISABLED', reason)); }
    for (const param of request.strategy.params) {
        const value = request.params[param.name] ?? param.default ?? '';
        if (param.required && !value.trim()) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', `${param.label} is required.`)); }
    }
    for (const key of Object.keys(request.params)) {
        if (!request.strategy.params.some(param => param.name === key)) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', `Unknown parameter ${key.slice(0, 40)}.`)); }
    }
    for (const key of Object.keys(request.scope)) { if (key !== 'pathPrefix') { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', `Unknown scope ${key.slice(0, 40)}.`)); } }
    relativePrefix(request.scope.pathPrefix);
}

export async function runStrategy(request: RunRequest, deps: RunDependencies): Promise<RunResult> {
    checkRequest(request);
    const { strategy } = request, budgets = strategy.budgets, now = deps.now ?? Date.now;
    const startedAt = now(), deadline = startedAt + budgets.seconds * 1000;
    const params: Record<string, string> = Object.fromEntries(strategy.params.map(param => [param.name, (request.params[param.name] ?? param.default ?? '').trim()]));
    const scope: Record<string, string> = request.scope.pathPrefix?.trim() ? { pathPrefix: relativePrefix(request.scope.pathPrefix)! } : {};
    const outputs = new Map<string, StepOutput>(), reports: StepReport[] = [], reasons: string[] = [];
    const sources = new Map<string, { root: string; relative: string; contentHash?: string; lines?: number }>();
    let ledger: LedgerRecord[] = [], calls = 0, publication: string | undefined, serviceVersion: string | undefined;
    const checked = new Map<string, LiveCheck>();

    const call = async (tool: NativeTool, args: Value): Promise<ToolResult> => {
        if (calls >= budgets.calls) { reasons.push(`call budget of ${budgets.calls} reached`); throw new Stop(); }
        if (now() > deadline) { reasons.push(`time budget of ${budgets.seconds} s reached`); throw new Stop(); }
        calls++;
        const result = await deps.session.call(tool, args, { timeout: Math.max(1000, Math.min(30000, deadline - now())) });
        if (tool !== 'airk_status' && !publication) { reasons.push('evidence was requested before the publication was pinned'); throw new Stop(); }
        if (publication && result.publication && result.publication !== publication) {
            // Fail closed: evidence from two publications is never combined.
            reasons.push(`the index was republished during the run (${result.publication})`); throw new Stop();
        }
        return result;
    };
    const record = (step: StrategyStep, entry: Omit<LedgerRecord, 'step'>) => { ledger.push({ ...entry, step: step.id }); };
    const factsFor = (step: StrategyStep, tool: NativeTool, args: Value, payload: Value, pub: string | undefined, output: StepOutput) => {
        const view = factList({ tool, args, limit: Number(args.limit ?? 50) }, payload, pub, deps.project, deps.facts);
        for (const item of view.items) {
            const hash = item.ref ? deps.facts.refs.get(item.ref, deps.project).contentHash : undefined;
            if (item.symbolRef) { output.symbols.push(deps.facts.symbols.get(item.symbolRef, deps.project).symbolId); }
            if (!item.startLine || !item.endLine) { continue; }
            output.locations.push({ path: item.path, startLine: item.startLine, endLine: item.endLine, ...(hash ? { contentHash: hash } : {}) });
            const text = [item.signature ?? item.qualifiedName ?? item.name, item.detail].filter(Boolean).join('\n').slice(0, 2000);
            if (hash) { record(step, { path: item.path, startLine: item.startLine, endLine: item.endLine, contentHash: hash, producer: tool, evidenceClass: item.evidenceClass, text, truncated: view.truncated }); }
        }
        if (view.truncated) { reasons.push(`${step.id}: ${tool} reported truncated results`); }
    };
    const args = (step: StrategyStep): Value => {
        const bound: Value = {};
        for (const [key, value] of Object.entries(step.args)) {
            if (typeof value === 'number') { bound[key] = value; continue; }
            const text = bind(value, params, scope).trim();
            if (!text) { continue; }
            bound[key] = key === 'path_prefix' ? relativePrefix(text) : text;
        }
        return bound;
    };
    const fanout = <T>(step: StrategyStep, items: T[]): T[] => {
        if (items.length > budgets.fanout) { reasons.push(`${step.id}: fan-out limited to ${budgets.fanout} of ${items.length}`); }
        return items.slice(0, budgets.fanout);
    };
    const pool = async <T>(items: T[], work: (item: T) => Promise<void>) => {
        let next = 0;
        let failed = false;
        const errors: { index: number; error: unknown }[] = [];
        const worker = async () => {
            while (!failed && next < items.length) {
                const index = next++;
                try { await work(items[index]); }
                catch (error) { failed = true; errors.push({ index, error }); }
            }
        };
        await Promise.all(Array.from({ length: Math.min(budgets.concurrency, 3, items.length) }, worker));
        if (errors.length) { throw errors.sort((a, b) => a.index - b.index)[0].error; }
    };
    const source = async (path: string) => {
        if (!sources.has(path)) {
            const info = (await call('airk_file_info', { path })).payload;
            const located = info.source as Value | undefined;
            const hash = info.content_hash, lines = info.indexed_lines;
            if (located && typeof located.root === 'string' && typeof located.relative_path === 'string') {
                sources.set(path, { root: located.root, relative: located.relative_path, ...(typeof hash === 'string' ? { contentHash: hash } : {}),
                    ...(typeof lines === 'number' && Number.isSafeInteger(lines) && lines > 0 ? { lines } : {}) });
            }
        }
        return sources.get(path);
    };

    let stopped = false;
    for (const step of strategy.steps) {
        const spec = operator(step.op)!;
        const output: StepOutput = { locations: [], symbols: [] };
        const before = calls, recordsBefore = ledger.length;
        if (stopped) { reports.push({ id: step.id, op: step.op, calls: 0, records: 0, outcome: 'skipped' }); continue; }
        try {
            const input = step.input ? outputs.get(step.input) ?? { locations: [], symbols: [] } : undefined;
            switch (step.op) {
                case 'airk.status': case 'airk.capabilities': {
                    const result = await call(spec.tool as NativeTool, {});
                    if (step.op === 'airk.status') {
                        publication ??= result.publication;
                        if (typeof result.payload.version === 'string') { serviceVersion = result.payload.version.slice(0, 64); }
                        if (!publication) { reasons.push('AIRK did not report a publication token'); throw new Stop(); }
                    }
                    break;
                }
                case 'airk.search': {
                    const bound = args(step);
                    const message = { v: 1, type: 'search.run', requestId: 'strategy', query: String(bound.query), mode: 'lexical', limit: Number(bound.limit ?? 20),
                        maxBytes: 16384, fullChunks: false, ...(bound.path_prefix ? { pathPrefix: String(bound.path_prefix) } : {}) } as const;
                    const result = await call('airk_search', { query: bound.query, mode: 'lexical', limit: message.limit, max_bytes: 16384, full_chunks: false,
                        ...(bound.path_prefix ? { path_prefix: bound.path_prefix } : {}) });
                    const view = searchView(deps.project, message, result.payload, result.publication, deps.facts.refs, {});
                    for (const hit of view.hits) {
                        output.locations.push({ path: hit.path, startLine: hit.startLine, endLine: hit.endLine, contentHash: hit.contentHash });
                        record(step, { path: hit.path, startLine: hit.startLine, endLine: hit.endLine, contentHash: hit.contentHash, producer: 'airk_search', evidenceClass: 'E1', text: hit.text, truncated: hit.truncated });
                    }
                    if (view.truncated) { reasons.push(`${step.id}: search results were truncated`); }
                    break;
                }
                case 'airk.listFiles': {
                    // airk_list_files takes `prefix` and answers with a page of plain `paths`.
                    const { path_prefix: prefix, ...rest } = args(step);
                    const result = await call('airk_list_files', { ...rest, ...(prefix ? { prefix } : {}), max_bytes: 16384 });
                    for (const path of Array.isArray(result.payload.paths) ? result.payload.paths : []) {
                        if (typeof path === 'string' && path.length <= 4096) { output.locations.push({ path, startLine: 1, endLine: 1 }); }
                    }
                    if (result.payload.truncated === true || typeof result.payload.cursor === 'string') { reasons.push(`${step.id}: more files exist than were listed`); }
                    break;
                }
                case 'airk.symbols': case 'airk.imports': case 'airk.codeFacts': case 'airk.references': case 'airk.documentFacts': {
                    const bound = { ...args(step), max_bytes: 16384 };
                    const result = await call(spec.tool as NativeTool, bound);
                    factsFor(step, spec.tool as NativeTool, bound, result.payload, result.publication, output);
                    break;
                }
                case 'airk.changes': {
                    const result = await call('airk_changes', { ...args(step), max_bytes: 16384 });
                    const items = Array.isArray(result.payload.changes) ? result.payload.changes.length : Array.isArray(result.payload.items) ? result.payload.items.length : 0;
                    reports.push({ id: step.id, op: step.op, calls: calls - before, records: 0, outcome: 'ok', message: `${items} change record(s)` });
                    outputs.set(step.id, output);
                    continue;
                }
                case 'airk.fileInfo': {
                    await pool(fanout(step, [...new Set(input!.locations.map(location => location.path))]), async path => {
                        const located = await source(path);
                        if (located) { output.locations.push({ path, startLine: 1, endLine: 1, ...(located.contentHash ? { contentHash: located.contentHash } : {}) }); }
                    });
                    break;
                }
                case 'airk.outline': {
                    await pool(fanout(step, [...new Set(input!.locations.map(location => location.path))]), async path => {
                        const bound = { path, limit: Number(step.args.limit ?? 40), max_bytes: 16384 };
                        const result = await call('airk_outline', bound);
                        factsFor(step, 'airk_outline', bound, result.payload, result.publication, output);
                    });
                    break;
                }
                case 'airk.readFile': {
                    const context = Number(step.args.context ?? 0);
                    const unique = [...new Map(input!.locations.map(location => [`${location.path}:${location.startLine}:${location.endLine}`, location])).values()];
                    await pool(fanout(step, unique), async location => {
                        // AIRK refuses ranges past the indexed line count, so clamp to the file record.
                        const lines = (await source(location.path))?.lines;
                        const wanted = location.startLine === 1 && location.endLine === 1 && !location.contentHash ? 40 : location.endLine + context;
                        const end = Math.max(location.endLine, lines ? Math.min(lines, wanted) : location.endLine);
                        const result = await call('airk_read_file', { path: location.path, start_line: Math.max(1, location.startLine - context), end_line: end, max_bytes: 8192 });
                        const view = searchView(deps.project, { v: 1, type: 'search.run', requestId: 'strategy', query: '', mode: 'lexical', limit: 1, maxBytes: 8192, fullChunks: false },
                            result.payload, result.publication, deps.facts.refs, {});
                        for (const hit of view.hits) {
                            output.locations.push({ path: hit.path, startLine: hit.startLine, endLine: hit.endLine, contentHash: hit.contentHash });
                            record(step, { path: hit.path, startLine: hit.startLine, endLine: hit.endLine, contentHash: hit.contentHash, producer: 'airk_read_file', evidenceClass: 'E1', text: hit.text, truncated: hit.truncated || result.payload.truncated === true });
                        }
                    });
                    break;
                }
                case 'airk.relationships': {
                    await pool(fanout(step, [...new Set(input!.symbols)]), async symbol => {
                        const bound = { symbol, direction: step.args.direction ?? 'incoming', hops: Number(step.args.hops ?? 1), limit: Number(step.args.limit ?? 20), max_bytes: 16384 };
                        const result = await call('airk_relationships', bound);
                        factsFor(step, 'airk_relationships', bound, result.payload, result.publication, output);
                    });
                    break;
                }
                case 'host.hashSavedFiles': {
                    // Every cited file collected so far, not only this step's input, so the outcome covers the whole ledger.
                    const paths = [...new Set([...input!.locations.map(location => location.path), ...ledger.map(entry => entry.path)])].sort();
                    await pool(fanout(step, paths), async path => {
                        const located = await source(path);
                        const hash = ledger.find(entry => entry.path === path)?.contentHash ?? located?.contentHash;
                        checked.set(path, located && hash ? await deps.live(located.root, located.relative, hash) : 'unchecked');
                    });
                    break;
                }
                case 'core.dedupe': case 'core.sort': ledger = orderRecords(ledger); break;
                case 'core.clip': {
                    const clipped = clipRecords(orderRecords(ledger), Number(step.args.bytes ?? budgets.evidenceBytes));
                    if (clipped.omitted) { reasons.push(`${step.id}: ${clipped.omitted} record(s) clipped`); }
                    ledger = clipped.kept; break;
                }
                case 'core.cite': ledger = ledger.filter(entry => /^[0-9a-f]{64}$/.test(entry.contentHash) && entry.startLine >= 1); break;
                case 'core.renderPrompt': break;
                default: throw new EvidenceFailure(explorerError('CAPABILITY_DISABLED', `${step.op} cannot run in this version.`));
            }
            reports.push({ id: step.id, op: step.op, calls: calls - before, records: ledger.length - recordsBefore, outcome: 'ok' });
        } catch (error) {
            if (error instanceof Stop) { stopped = true; reports.push({ id: step.id, op: step.op, calls: calls - before, records: ledger.length - recordsBefore, outcome: 'error', message: reasons.at(-1) }); }
            else {
                const message = error instanceof EvidenceFailure ? `${error.error.code}: ${error.error.message}` : error instanceof Error ? error.message : String(error);
                reasons.push(`${step.id} failed: ${message.slice(0, 300)}`);
                reports.push({ id: step.id, op: step.op, calls: calls - before, records: ledger.length - recordsBefore, outcome: 'error', message: message.slice(0, 300) });
                if (step.op === 'airk.status') { stopped = true; }
            }
        }
        outputs.set(step.id, output);
    }

    const ordered = orderRecords(ledger);
    const { kept, omitted } = clipRecords(ordered, budgets.evidenceBytes);
    if (omitted) { reasons.push(`the ${budgets.evidenceBytes}-byte evidence budget left out ${omitted} record(s)`); }
    if (kept.some(entry => entry.truncated)) { reasons.push('some excerpts are truncated'); }
    const cited = [...new Set(kept.map(entry => entry.path))];
    const mismatched = cited.filter(path => checked.has(path) && checked.get(path) !== 'matches_index');
    const unchecked = cited.filter(path => !checked.has(path));
    for (const path of mismatched) { reasons.push(`live check: ${path} is ${checked.get(path)!.replaceAll('_', ' ')}`); }
    if (unchecked.length) { reasons.push(`live files not checked for ${unchecked.length} cited path(s)`); }
    if (!kept.length) { reasons.push('no evidence was collected'); }
    const digest = createHash('sha256').update(JSON.stringify(kept.map(entry => [entry.path, entry.startLine, entry.endLine, entry.contentHash, entry.producer, entry.text]))).digest('hex');
    return { records: kept, omitted, manifest: {
        schema: RUN_SCHEMA, extensionVersion: deps.extensionVersion, ...(serviceVersion ? { serviceVersion } : {}), project: deps.project, publication: publication ?? 'unknown',
        template: `${strategy.id}@${strategy.version}`, params, scope, steps: reports, evidenceDigest: digest,
        outcome: reasons.length || !publication ? 'partial' : 'verified_snapshot', reasons: [...new Set(reasons)].slice(0, 40), startedAt, finishedAt: now(),
    } };
}
