// SPDX-License-Identifier: Apache-2.0
import { createHash, randomUUID } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { LaunchSpec, launchArgs } from './model';
import { NativeProcessError, runText } from './native';

export interface DiagnosticError {
    kind: 'native_process' | 'invalid_response' | 'error'; message: string;
    code?: string | number | null; signal?: string | null; killed?: boolean; timedOut?: boolean;
    elapsedMs?: number; timeoutMs?: number; stderr?: string;
}
export interface DiagnosticProbe {
    state: 'ok' | 'busy' | 'failed' | 'unknown'; checkedAt: string; durationMs: number;
    value?: Record<string, unknown> | null; error?: DiagnosticError; reason?: 'not_indexed';
}
export type DiagnosticEventKind = 'helper' | 'index_failed' | 'semantic_failed' | 'repair' | 'index_completed';
export interface DiagnosticEvent {
    at: string; kind: DiagnosticEventKind; helperPath: string; helperVersion?: string;
    error?: DiagnosticError; action?: string;
}
export interface DiagnosticReport {
    schemaVersion: 1; capturedAt: string;
    binding: { helperPath: string; root: string; dataDir: string; collection: string; name: string };
    probes: Record<'helper' | 'status' | 'schedule' | 'vectors' | 'extraction' | 'storage' | 'integrity', DiagnosticProbe>;
    assessment: {
        state: 'ready' | 'building' | 'paused' | 'blocked' | 'failed' | 'busy' | 'not_indexed' | 'unknown';
        summary: string; recommendedAction: 'none' | 'wait' | 'resume' | 'retry' | 'rebuild' | 'review';
    };
    recentEvents: DiagnosticEvent[];
    editor?: DiagnosticEditorState;
    historyError?: DiagnosticError;
}
export interface DiagnosticEditorState { paused: boolean; state?: string; pending?: boolean; busy: boolean }
export type DiagnosticRunner = (spec: LaunchSpec, command: string, extra: string[], timeoutMs: number) => Promise<unknown>;
export interface DiagnosticOptions {
    expectedVersion?: string; runner?: DiagnosticRunner; events?: DiagnosticEvent[]; helperError?: unknown; editor?: DiagnosticEditorState; historyError?: unknown;
    shouldContinue?: () => boolean;
}
export interface IntegrityVerificationOptions {
    runner?: DiagnosticRunner; maxMs?: number; shouldContinue?: () => boolean;
    onProgress?: (report: DiagnosticReport) => void | Promise<void>;
}

const PROBE_TIMEOUT_MS = 10000, MAX_EVENTS = 24, MAX_HISTORY_BYTES = 128 * 1024;
const eventKinds: DiagnosticEventKind[] = ['helper', 'index_failed', 'semantic_failed', 'repair', 'index_completed'];
const bounded = (value: string, limit: number) => value.length <= limit ? value : value.slice(0, limit) + '\n[truncated]';
const object = (value: unknown): Record<string, unknown> | undefined => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
class InvalidResponse extends Error {}

/** Paths and native error messages are included; source/query excerpts are never requested. */
export function diagnosticError(error: unknown): DiagnosticError {
    const result: DiagnosticError = { kind: error instanceof NativeProcessError ? 'native_process'
        : error instanceof InvalidResponse ? 'invalid_response' : 'error', message: bounded(error instanceof Error ? error.message : String(error), 1024) };
    if (error instanceof NativeProcessError) {
        result.code = error.code; result.signal = error.signal; result.killed = error.killed; result.timedOut = error.timedOut;
        result.elapsedMs = error.elapsedMs; result.timeoutMs = error.timeoutMs; result.stderr = bounded(error.stderr, 2048);
    }
    return result;
}

function nativeReadReason(error: unknown): 'busy' | 'not_indexed' | undefined {
    if (!(error instanceof NativeProcessError) || error.code !== 1 || error.killed || error.signal) { return undefined; }
    if (/^(?:Error: )?current index is busy applying an update(?:\s*\r?\n\s*Caused by:\s*\r?\n\s*|:\s*)lock acquisition failed because the operation would block\s*$/.test(error.stderr)) { return 'busy'; }
    if (/^(?:Error: )?workspace is not indexed(?:; run airk index first)?\s*$/.test(error.stderr)) { return 'not_indexed'; }
    return undefined;
}

function fields(value: unknown, names: string[]): Record<string, unknown> {
    const input = object(value), result: Record<string, unknown> = {};
    if (!input) { return result; }
    for (const key of names) {
        const field = input[key];
        if (field === null || typeof field === 'boolean' || (typeof field === 'number' && Number.isFinite(field) && field >= 0)) { result[key] = field; }
        else if (typeof field === 'string') { result[key] = bounded(field, key === 'failure' || key === 'error' || key === 'scan_error' ? 2048 : 256); }
    }
    return result;
}
function readiness(value: unknown): Record<string, unknown> | undefined {
    const input = object(value);
    if (!input || !['pending', 'partial', 'ready', 'failed', 'disabled', 'paused'].includes(String(input.state))) { return undefined; }
    return fields(input, ['state', 'selected_artifacts', 'completed_artifacts', 'selected_units', 'completed_units',
        'processed_artifacts', 'committed_artifacts', 'total_artifacts', 'first_ready_unix_ms', 'fully_ready_unix_ms', 'failure']);
}
function withReadiness(input: Record<string, unknown>, result: Record<string, unknown>): Record<string, unknown> {
    for (const key of ['semantic_readiness', 'discovery_readiness']) { const ready = readiness(input[key]); if (ready) { result[key] = ready; } }
    return result;
}
function requireObject(value: unknown): Record<string, unknown> {
    const input = object(value); if (!input) { throw new InvalidResponse('Native diagnostic response was not an object.'); } return input;
}
function normalizedPath(value: string): string {
    const resolved = path.resolve(value);
    // Rust canonical paths on Windows carry the extended prefix; Node's
    // workspace realpath commonly does not. Normalize drive and UNC spellings.
    return process.platform === 'win32' ? path.toNamespacedPath(resolved).toLowerCase() : resolved;
}
function samePath(left: unknown, right: string): boolean {
    if (typeof left !== 'string') { return false; }
    return normalizedPath(left) === normalizedPath(right);
}

function selectProbe(name: keyof DiagnosticReport['probes'], value: unknown, spec: LaunchSpec, expectedVersion?: string): Record<string, unknown> | null {
    if (name === 'helper') {
        const version = typeof value === 'string' && /^airk ([0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?)\s*$/.exec(value)?.[1];
        if (!version) { throw new InvalidResponse('Helper did not return a recognizable AIRK version.'); }
        return { version, ...(expectedVersion ? { expectedVersion, matchesExtension: version === expectedVersion } : {}) };
    }
    if (name === 'integrity') {
        const input = requireObject(value), publication = object(input.publication);
        const counts = ['total_files', 'checked_files', 'verified_sources', 'verified_extractions', 'checked_source_bytes', 'checked_extraction_bytes', 'failed_objects', 'failures_truncated', 'skipped_large'];
        if (publication?.collection !== spec.collection || typeof publication.token !== 'string' || typeof input.complete !== 'boolean'
            || typeof input.from_start !== 'boolean' || !counts.every(key => typeof input[key] === 'number' && Number.isSafeInteger(input[key]) && Number(input[key]) >= 0)
            || !Array.isArray(input.failures) || input.failures.length > 8 || input.failures.length + Number(input.failures_truncated) !== input.failed_objects
            || Number(input.checked_files) + Number(input.skipped_large) > 128 || Number(input.checked_files) > Number(input.total_files)
            || Number(input.verified_sources) > Number(input.checked_files) || Number(input.verified_extractions) > Number(input.checked_files)
            || (input.complete ? input.next_after !== null : typeof input.next_after !== 'string' || !input.next_after || input.next_after.length > 4096)
            || !input.failures.every(failure => { const entry = object(failure); return entry && typeof entry.path === 'string' && entry.path.length <= 4096
                && ['source', 'extraction', 'catalog'].includes(String(entry.component)) && ['missing', 'invalid', 'unavailable'].includes(String(entry.kind)); })) {
            throw new InvalidResponse('Native retained integrity state was incomplete or belonged to another collection.');
        }
        const result = fields(input, ['from_start', 'total_files', 'checked_files', 'verified_sources', 'verified_extractions', 'checked_source_bytes',
            'checked_extraction_bytes', 'failed_objects', 'failures_truncated', 'skipped_large', 'complete', 'next_after']);
        result.publication = fields(publication, ['collection', 'generation', 'incarnation', 'revision', 'token']);
        result.failures = input.failures.map(failure => ({ ...fields(failure, ['component', 'kind']), path: object(failure)!.path }));
        // Continuation paths are opaque native paths, bounded by the native catalog contract.
        if (typeof input.next_after === 'string') { result.next_after = bounded(input.next_after, 4096); }
        return result;
    }
    if (name === 'schedule' && value === null) { return null; }
    if (name === 'storage') {
        if (!Array.isArray(value)) { throw new InvalidResponse('Native collection storage response was not a list.'); }
        const matched = value.find(entry => object(entry)?.collection === spec.collection);
        return matched ? fields(matched, ['collection', 'status', 'disk_bytes', 'indexed_files', 'generation']) : null;
    }
    const input = requireObject(value);
    if (name === 'status') {
        if (input.collection !== spec.collection || typeof input.files !== 'number' || typeof input.generation !== 'number') { throw new InvalidResponse('Native status did not match the selected collection.'); }
        const result = withReadiness(input, fields(input, ['collection', 'generation', 'files', 'chunks', 'source_bytes', 'indexed_vectors',
            'local_model_ready', 'vectors_enabled', 'vector_eligible_artifacts', 'vector_excluded_artifacts', 'extractor_version']));
        if (input.publication === null) { result.publication = null; }
        else { result.publication = fields(input.publication, ['collection', 'generation', 'incarnation', 'revision', 'token']); }
        result.execution = fields(input.execution, ['mode', 'logical_cpus', 'requested_inference_workers', 'inference_workers', 'working_memory_bytes', 'resource_slots']);
        return result;
    }
    if (name === 'schedule') {
        if (input.collection !== spec.collection || !samePath(input.root, spec.root) || typeof input.paused !== 'boolean' || typeof input.blocked !== 'boolean') { throw new InvalidResponse('Native scheduler state did not match the selected collection.'); }
        return fields(input, ['collection', 'requested_revision', 'completed_revision', 'running_revision', 'paused', 'blocked', 'full', 'manual',
            'first_pending_unix_ms', 'last_event_unix_ms', 'last_completed_unix_ms', 'error', 'scan_started_unix_ms', 'scan_completed_unix_ms', 'scan_error']);
    }
    if (name === 'vectors') {
        if (typeof input.enabled !== 'boolean' || typeof input.published !== 'boolean') { throw new InvalidResponse('Native vector state was incomplete.'); }
        return withReadiness(input, fields(input, ['enabled', 'published', 'indexed_vectors', 'pending_reindex', 'selection_pending_reindex', 'default_search_mode']));
    }
    if (!['agent_lean', 'essential', 'balanced', 'detailed'].includes(String(input.configured_detail))) { throw new InvalidResponse('Native extraction state was incomplete.'); }
    return fields(input, ['configured_detail', 'published_detail', 'pending_reindex', 'origin']);
}

function assess(probes: DiagnosticReport['probes'], editor?: DiagnosticEditorState): DiagnosticReport['assessment'] {
    const status = probes.status.value, schedule = probes.schedule.value;
    // Only status binds readiness to the publication verified by the integrity
    // probe. The independent vectors observation carries no revision token.
    const semantic = object(status?.semantic_readiness), discovery = object(status?.discovery_readiness);
    const set = (state: DiagnosticReport['assessment']['state'], recommendedAction: DiagnosticReport['assessment']['recommendedAction'], summary: string) => ({ state, recommendedAction, summary });
    if (probes.helper.state !== 'ok' || probes.helper.value?.matchesExtension === false) { return set(probes.helper.state === 'unknown' ? 'unknown' : 'failed', 'review', 'The configured helper could not be verified. Review its version and installation before indexing.'); }
    const integrity = probes.integrity.value;
    if (probes.integrity.state === 'ok' && typeof integrity?.failed_objects === 'number' && integrity.failed_objects > 0) {
        const failures = Array.isArray(integrity.failures) ? integrity.failures : [];
        const missing = failures.length > 0 && failures.every(failure => object(failure)?.kind === 'missing') && integrity.failures_truncated === 0;
        return set('failed', missing ? 'retry' : 'review', missing
            ? 'Retained source or extraction inputs are missing. Retry indexing repairs eligible derived inputs from saved files.'
            : 'Retained inputs failed verification. Review the affected components; a full rebuild is an explicit option, not an automatic deletion.');
    }
    const failures = [discovery?.state === 'failed' ? discovery.failure : undefined, semantic?.state === 'failed' ? semantic.failure : undefined,
        schedule?.blocked ? schedule.error : undefined, ...Object.values(probes).filter(probe => probe.state === 'failed').map(probe => probe.error?.stderr || probe.error?.message)].filter(value => typeof value === 'string').join('\n');
    if (failures || discovery?.state === 'failed' || semantic?.state === 'failed' || schedule?.blocked) {
        const state = schedule?.blocked ? 'blocked' : 'failed';
        if (/retained object missing|previous generation cannot be reconstructed/i.test(failures)) { return set(state, 'retry', 'The current observation reports missing retained index data. Retry indexing repairs eligible derived inputs from saved files.'); }
        if (/quota|disk is full|insufficient.*space|permission denied|access is denied/i.test(failures)) { return set(state, 'review', 'Indexing reports a storage or access problem. Review the current errors before retrying.'); }
        return set(state, 'retry', 'A current indexing or diagnostic operation failed. Review its details, then retry; no state has been deleted.');
    }
    if (schedule?.paused || discovery?.state === 'paused' || editor?.paused || editor?.state === 'paused') { return set('paused', 'resume', 'Indexing is paused. Resume allows already requested work to continue.'); }
    if (editor?.state === 'failed' || editor?.state === 'capacity-limited') { return set('blocked', editor.state === 'capacity-limited' ? 'review' : 'retry', 'The editor stopped its current indexing request. Review its recorded error before retrying.'); }
    if (Object.values(probes).some(probe => probe.state === 'busy')) { return set('busy', 'wait', 'A native publication is holding the index. Refresh after the current batch finishes.'); }
    if (probes.status.reason === 'not_indexed') { return set('not_indexed', 'retry', 'This collection has no published index yet. Index now starts it.'); }
    if (probes.status.state !== 'ok') { return set('unknown', 'review', 'Current index state could not be established. Historical errors do not establish a current failure.'); }
    if (['pending', 'partial'].includes(String(semantic?.state)) && probes.schedule.state === 'ok' && schedule === null) {
        return set('blocked', 'retry', 'Semantic work is pending without a scheduler registration. Index now registers the collection and continues requested work.');
    }
    if (probes.vectors.value?.pending_reindex || probes.extraction.value?.pending_reindex) {
        return set('building', 'retry', 'A saved vector or evidence-detail change is awaiting publication. Index now applies it.');
    }
    if (editor?.busy || editor?.pending || ['pending', 'partial'].includes(String(discovery?.state)) || ['pending', 'partial'].includes(String(semantic?.state))) {
        return set('building', 'wait', 'Committed indexing work is incomplete with no current reported failure. One observation cannot establish whether a helper is advancing.');
    }
    if (Object.values(probes).some(probe => probe.state === 'unknown')) { return set('unknown', 'review', 'Some current diagnostic observations are unavailable. Review the individual results.'); }
    if (discovery?.state === 'ready' && ['ready', 'disabled'].includes(String(semantic?.state))) {
        if (object(integrity?.publication)?.token !== object(status?.publication)?.token || integrity?.total_files !== status?.files) {
            return set('unknown', 'review', 'The status and retained verification describe different publications. Refresh diagnostics to compare one current revision.');
        }
        if (integrity?.complete && integrity.from_start && integrity.skipped_large === 0 && integrity.checked_files === integrity.total_files
            && integrity.verified_sources === integrity.checked_files && integrity.verified_extractions === integrity.checked_files) {
            return set('ready', 'none', 'Published discovery and semantic states are ready, and all retained source and extraction inputs were verified. Earlier failures are historical.');
        }
        return set('unknown', 'review', 'Published indexing states are ready; retained input verification is partial. Verify retained data to check the remaining inputs.');
    }
    return set('unknown', 'review', 'The available observations do not establish complete index readiness.');
}

const defaultRunner: DiagnosticRunner = async (spec, command, extra, timeout) => {
    const text = await runText(spec.command, command === '--version' ? ['--version'] : [...launchArgs(spec, command), ...extra], spec.root, timeout);
    return command === '--version' ? text : JSON.parse(text);
};

/** Seven bounded observations, at most three helpers at once; never scans or indexes sources. */
export async function collectDiagnostics(spec: LaunchSpec, options: DiagnosticOptions = {}): Promise<DiagnosticReport> {
    const capturedAt = new Date().toISOString();
    const names: (keyof DiagnosticReport['probes'])[] = ['helper', 'status', 'schedule', 'vectors', 'extraction', 'storage', 'integrity'];
    const probes = Object.fromEntries(names.map(name => [name, { state: 'unknown', checkedAt: capturedAt, durationMs: 0 }])) as DiagnosticReport['probes'];
    const commands: Record<keyof DiagnosticReport['probes'], [string, string[]]> = { helper: ['--version', []], status: ['status', []],
        schedule: ['schedule', ['status']], vectors: ['collections', ['vectors', 'show']], extraction: ['collections', ['extraction', 'show']], storage: ['collections', ['list']], integrity: ['collections', ['diagnose']] };
    if (options.helperError !== undefined) { probes.helper = { ...probes.helper, state: 'failed', error: diagnosticError(options.helperError) }; }
    else {
        const runner = options.runner ?? defaultRunner;
        for (let offset = 0; offset < names.length; offset += 3) {
            if (options.shouldContinue?.() === false) { break; }
            await Promise.all(names.slice(offset, offset + 3).map(async name => {
                if (options.shouldContinue?.() === false) { return; }
                const checkedAt = new Date().toISOString(), started = performance.now();
                try {
                    const [command, extra] = commands[name];
                    const value = selectProbe(name, await runner(spec, command, extra, PROBE_TIMEOUT_MS), spec, options.expectedVersion);
                    probes[name] = { state: 'ok', checkedAt, durationMs: Math.round(performance.now() - started), value };
                } catch (error) {
                    const reason = nativeReadReason(error);
                    const unavailable = !(error instanceof NativeProcessError) || error.timedOut || error.code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER';
                    probes[name] = { state: reason === 'busy' ? 'busy' : reason === 'not_indexed' || unavailable ? 'unknown' : 'failed',
                        checkedAt, durationMs: Math.round(performance.now() - started), error: diagnosticError(error), ...(reason === 'not_indexed' ? { reason } : {}) };
                }
            }));
        }
    }
    return { schemaVersion: 1, capturedAt, binding: { helperPath: bounded(spec.command, 32768), root: bounded(spec.root, 32768),
        dataDir: bounded(spec.dataDir, 32768), collection: bounded(spec.collection, 256), name: bounded(spec.name, 512) },
        probes, assessment: assess(probes, options.editor), recentEvents: sanitizeEvents(options.events ?? []),
        ...(options.historyError === undefined ? {} : { historyError: diagnosticError(options.historyError) }), ...(options.editor ? { editor: {
            paused: !!options.editor.paused, busy: !!options.editor.busy, ...(typeof options.editor.state === 'string' ? { state: bounded(options.editor.state, 64) } : {}),
            ...(typeof options.editor.pending === 'boolean' ? { pending: options.editor.pending } : {}) } } : {}) };
}

/** Explicit full verification resumes bounded pages at one exact publication. */
export async function verifyDiagnosticIntegrity(spec: LaunchSpec, prior: DiagnosticReport, options: IntegrityVerificationOptions = {}): Promise<DiagnosticReport> {
    if (prior.binding.collection !== spec.collection || !samePath(prior.binding.root, spec.root) || !samePath(prior.binding.dataDir, spec.dataDir)
        || !samePath(prior.binding.helperPath, spec.command) || prior.probes.helper.state !== 'ok' || prior.probes.helper.value?.matchesExtension === false) {
        throw new Error('Verify retained data requires the current collection and a verified helper.');
    }
    const maxMs = options.maxMs ?? 120000;
    if (!Number.isFinite(maxMs) || maxMs < 1 || maxMs > 120000) { throw new Error('Retained verification budget must be between 1 and 120000 ms.'); }
    const runner = options.runner ?? defaultRunner, started = performance.now();
    let report = prior;
    let aggregate = prior.probes.integrity.state === 'ok' && prior.probes.integrity.value?.from_start && !prior.probes.integrity.value.complete
        && typeof prior.probes.integrity.value.next_after === 'string' ? { ...prior.probes.integrity.value } : undefined;
    const update = async (probe: DiagnosticProbe) => {
        const probes = { ...report.probes, integrity: probe };
        report = { ...report, capturedAt: new Date().toISOString(), probes, assessment: assess(probes, report.editor) };
        await options.onProgress?.(report);
    };
    const stop = async (reason: string) => {
        await update({ ...report.probes.integrity, state: aggregate ? 'ok' : 'unknown', value: {
            ...aggregate, complete: false, interrupted_reason: reason }, checkedAt: new Date().toISOString(), durationMs: Math.round(performance.now() - started) });
        return report;
    };
    for (;;) {
        if (options.shouldContinue?.() === false) { return stop('cancelled'); }
        const remaining = maxMs - (performance.now() - started);
        if (remaining < 1) { return stop('time_budget'); }
        const extra = ['diagnose'];
        const token = object(aggregate?.publication)?.token;
        if (aggregate) {
            if (typeof token !== 'string' || typeof aggregate.next_after !== 'string') { return stop('continuation_unavailable'); }
            extra.push('--revision', token, '--after', aggregate.next_after);
        }
        const checkedAt = new Date().toISOString(), pageStarted = performance.now();
        try {
            const page = selectProbe('integrity', await runner(spec, 'collections', extra, Math.max(1, Math.min(PROBE_TIMEOUT_MS, Math.floor(remaining)))), spec)!;
            if (aggregate) {
                if (object(page.publication)?.token !== token || page.total_files !== aggregate.total_files || page.from_start !== false
                    || (!page.complete && page.next_after === aggregate.next_after)) { throw new InvalidResponse('Retained verification page does not continue the selected publication.'); }
                const failures = [...(Array.isArray(aggregate.failures) ? aggregate.failures : []), ...(Array.isArray(page.failures) ? page.failures : [])].slice(0, 8);
                const combined: Record<string, unknown> = { ...page, from_start: true, failures };
                for (const field of ['checked_files', 'verified_sources', 'verified_extractions', 'checked_source_bytes', 'checked_extraction_bytes', 'failed_objects', 'skipped_large']) {
                    combined[field] = Number(aggregate[field] ?? 0) + Number(page[field] ?? 0);
                }
                combined.failures_truncated = Number(combined.failed_objects) - failures.length;
                aggregate = combined;
            } else { aggregate = page; }
            await update({ state: 'ok', checkedAt, durationMs: Math.round(performance.now() - pageStarted), value: aggregate });
            if (aggregate.complete) { return report; }
        } catch (error) {
            const reason = nativeReadReason(error);
            const expired = error instanceof NativeProcessError && error.code === 1 && !error.killed && !error.signal
                && /^Error: retained integrity revision changed; restart verification at the current head\s*$/.test(error.stderr);
            const unavailable = expired || !(error instanceof NativeProcessError) || error.timedOut || error.code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER';
            await update({ state: reason === 'busy' ? 'busy' : unavailable || reason === 'not_indexed' ? 'unknown' : 'failed', checkedAt,
                durationMs: Math.round(performance.now() - pageStarted), error: diagnosticError(error), value: {
                    ...aggregate, complete: false, ...(expired ? { next_after: null, interrupted_reason: 'revision_changed' } : {}) } });
            return report;
        }
    }
}

function sanitizeEvents(values: unknown): DiagnosticEvent[] {
    if (!Array.isArray(values)) { return []; }
    return values.slice(-MAX_EVENTS).flatMap(value => {
        const event = object(value);
        if (!event || !eventKinds.includes(event.kind as DiagnosticEventKind) || typeof event.at !== 'string' || !Number.isFinite(Date.parse(event.at)) || typeof event.helperPath !== 'string') { return []; }
        const clean: DiagnosticEvent = { at: new Date(event.at).toISOString(), kind: event.kind as DiagnosticEventKind, helperPath: bounded(event.helperPath, 4096) };
        if (typeof event.helperVersion === 'string') { clean.helperVersion = bounded(event.helperVersion, 128); }
        if (typeof event.action === 'string') { clean.action = bounded(event.action, 128); }
        const error = object(event.error);
        if (error && typeof error.message === 'string') {
            clean.error = { kind: ['native_process', 'invalid_response'].includes(String(error.kind)) ? error.kind as DiagnosticError['kind'] : 'error', message: bounded(error.message, 1024) };
            Object.assign(clean.error, fields(error, ['signal', 'killed', 'timedOut', 'elapsedMs', 'timeoutMs']));
            if (typeof error.code === 'number' && Number.isFinite(error.code)) { clean.error.code = error.code; }
            else if (typeof error.code === 'string') { clean.error.code = bounded(error.code, 256); }
            else if (error.code === null) { clean.error.code = null; }
            if (typeof error.stderr === 'string') { clean.error.stderr = bounded(error.stderr, 2048); }
        }
        return [clean];
    });
}

/** A separate per-binding bounded log survives editor reloads and helper upgrades. */
export class DiagnosticHistory {
    private tails = new Map<string, Promise<void>>();
    private closed = false;
    constructor(private readonly directory: string) {}
    /** Wait until accepted writes settle; failures remain visible to append callers. */
    async flush(): Promise<void> {
        while (this.tails.size) { await Promise.all([...this.tails.values()]); }
    }
    /** Stop admission synchronously, then settle already accepted writes. */
    close(): Promise<void> { this.closed = true; return this.flush(); }
    private binding(spec: LaunchSpec): string {
        return createHash('sha256').update(JSON.stringify([normalizedPath(spec.root), normalizedPath(spec.dataDir), spec.collection])).digest('hex');
    }
    private async readFile(spec: LaunchSpec): Promise<DiagnosticEvent[]> {
        const binding = this.binding(spec), file = path.join(this.directory, binding + '.json');
        let metadata;
        try { metadata = await fs.lstat(file); } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') { return []; } throw error; }
        if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > MAX_HISTORY_BYTES) { throw new Error('Diagnostic history is not a bounded regular file.'); }
        const handle = await fs.open(file, 'r');
        try {
            const buffer = Buffer.alloc(MAX_HISTORY_BYTES + 1); let size = 0;
            while (size < buffer.length) { const read = await handle.read(buffer, size, buffer.length - size, size); if (!read.bytesRead) { break; } size += read.bytesRead; }
            if (size > MAX_HISTORY_BYTES) { throw new Error('Diagnostic history exceeds its byte limit.'); }
            const saved = JSON.parse(buffer.subarray(0, size).toString('utf8'));
            if (saved.schemaVersion !== 1 || saved.binding !== binding) { throw new Error('Diagnostic history binding does not match this collection.'); }
            return sanitizeEvents(saved.events);
        } finally { await handle.close(); }
    }
    async read(spec: LaunchSpec): Promise<DiagnosticEvent[]> {
        await this.tails.get(this.binding(spec));
        return this.readFile(spec);
    }
    append(spec: LaunchSpec, event: { kind: DiagnosticEventKind; helperVersion?: string; error?: unknown; action?: string }): Promise<void> {
        if (this.closed) { return Promise.resolve(); }
        const binding = this.binding(spec);
        const result = (this.tails.get(binding) ?? Promise.resolve()).then(async () => {
            await fs.mkdir(this.directory, { recursive: true });
            if ((await fs.lstat(this.directory)).isSymbolicLink()) { throw new Error('Diagnostic history directory must not be redirected.'); }
            const events = sanitizeEvents([...(await this.readFile(spec)), { at: new Date().toISOString(), kind: event.kind, helperPath: spec.command,
                helperVersion: event.helperVersion, action: event.action, ...(event.error === undefined ? {} : { error: diagnosticError(event.error) }) }]);
            let bytes = JSON.stringify({ schemaVersion: 1, binding, events });
            while (Buffer.byteLength(bytes) > MAX_HISTORY_BYTES && events.length) { events.shift(); bytes = JSON.stringify({ schemaVersion: 1, binding, events }); }
            const temporary = path.join(this.directory, `${binding}-${randomUUID()}.tmp`);
            try { await fs.writeFile(temporary, bytes, { flag: 'wx', mode: 0o600 }); await fs.rename(temporary, path.join(this.directory, binding + '.json')); }
            finally { await fs.unlink(temporary).catch(error => { if (error.code !== 'ENOENT') { throw error; } }); }
        });
        const tail = result.then(() => undefined, () => undefined); this.tails.set(binding, tail);
        void tail.then(() => { if (this.tails.get(binding) === tail) { this.tails.delete(binding); } });
        return result;
    }
}
