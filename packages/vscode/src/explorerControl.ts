// SPDX-License-Identifier: Apache-2.0
import * as path from 'node:path';
import type { ConnectionState, ProjectSummary, SemanticState } from './explorerContracts';
import type { RecordValue } from './serviceClient';
import type { ServiceConsoleState } from './serviceConsoleView';

/**
 * The Explorer's control plane is the existing Service Console controller: one native
 * control connection, one intent journal and one polling loop are shared, so journaled
 * retries, confirmations and page-scoped validation behave exactly as they do today.
 */
export interface ControlSource {
    readonly controller: { readonly state: ServiceConsoleState };
    run(action: string, fields?: Record<string, unknown>): Promise<void>;
    retain(): { dispose(): void };
    onDidChangeState(listener: (state: ServiceConsoleState) => void): { dispose(): void };
}

const obj = (value: unknown): RecordValue => value && typeof value === 'object' && !Array.isArray(value) ? value as RecordValue : {};
const text = (value: unknown): string | undefined => typeof value === 'string' && value.length <= 4096 ? value : undefined;
const count = (value: unknown): number | undefined => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
const SEMANTIC: readonly SemanticState[] = ['pending', 'partial', 'ready', 'failed', 'disabled'];

/** Cached console page rows → project summaries. Missing observations stay undefined, never zero. */
export function projectSummaries(state: ServiceConsoleState): ProjectSummary[] {
    if (state.inactiveProjects) { return []; }
    return (state.page?.projects ?? []).flatMap(row => {
        const project = obj(row.project), registration = obj(project.registration), binding = text(registration.binding);
        if (!binding || !/^[0-9a-f]{64}$/.test(binding)) { return []; }
        const inspection = obj(row.inspection), snapshot = obj(inspection.snapshot), semantic = obj(snapshot.semantic);
        const progress = obj(row.progress), job = obj(progress.job), work = obj(progress.progress), watch = obj(row.watch);
        const root = text(registration.root) ?? '';
        const semanticState = SEMANTIC.find(value => value === semantic.state);
        const summary: ProjectSummary = {
            project: binding,
            collection: text(registration.collection) ?? binding.slice(0, 12),
            name: root ? path.basename(root) || root : text(registration.collection) ?? binding.slice(0, 12),
            root,
            current: state.currentProjects?.has(binding) === true,
            ...(typeof project.paused === 'boolean' ? { paused: project.paused } : {}),
            ...(typeof watch.enabled === 'boolean' ? { watching: watch.enabled } : {}),
            ...(text(snapshot.publication) ? { publication: text(snapshot.publication) } : {}),
            ...(count(snapshot.indexed_files) !== undefined ? { indexedFiles: count(snapshot.indexed_files) } : {}),
            ...(semanticState ? { semantic: semanticState } : {}),
            ...(text(snapshot.index_state) ? { indexState: text(snapshot.index_state) } : {}),
            ...(text(job.state) ? { jobState: text(job.state) } : {}),
            ...(text(job.kind) ? { jobKind: text(job.kind) } : {}),
            ...(text(work.phase) ? { phase: text(work.phase) } : {}),
            ...(count(inspection.observed_unix_ms) !== undefined ? { observedAt: count(inspection.observed_unix_ms) } : {}),
        };
        return [summary];
    });
}

/** Service connection only; published readiness and freshness are reported separately. */
export function connectionOf(state: ServiceConsoleState, extensionVersion: string): ConnectionState {
    if (!state.trusted) { return 'untrusted'; }
    const selected = state.bootstrap.value?.selectedVersion;
    if (typeof selected === 'string' && selected !== extensionVersion) { return 'incompatible'; }
    if (state.connected) {
        const accepting = obj(obj(state.health.value?.observation).policy).accepting;
        return accepting === false ? 'degraded' : 'ready';
    }
    if (state.health.state === 'unavailable' && state.busy) { return 'connecting'; }
    return 'disconnected';
}

export class ExplorerControl {
    constructor(private readonly source: ControlSource, private readonly extensionVersion: string) {}
    get state(): ServiceConsoleState { return this.source.controller.state; }
    projects(): ProjectSummary[] { return projectSummaries(this.state); }
    connection(): ConnectionState { return connectionOf(this.state, this.extensionVersion); }
    observedAt(): number | undefined { return this.state.page?.observed_unix_ms; }
    message(): string | undefined { return this.state.message; }
    retain(): { dispose(): void } { return this.source.retain(); }
    onDidChange(listener: () => void): { dispose(): void } { return this.source.onDidChangeState(() => listener()); }
    refresh(): Promise<void> { return this.source.run('refresh'); }
    /** Journaled native index job for one project on the current page. */
    index(project: string): Promise<void> { return this.source.run('index', { project }); }
}
