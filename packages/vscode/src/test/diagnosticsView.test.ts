// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import Module = require('node:module');
import type { DiagnosticReport } from '../diagnostics';
import { diagnosticsAction, openDiagnosticsPanel, renderDiagnosticsReport } from '../diagnosticsView';

function fixture(): DiagnosticReport {
    const probe = { state: 'ok' as const, checkedAt: '2026-10-20T20:00:00.000Z', durationMs: 12, value: {} };
    return {
        schemaVersion: 1, capturedAt: probe.checkedAt,
        binding: { name: 'Fixture', root: '/workspace', dataDir: '/indexes', collection: 'fixture', helperPath: '/helpers/airk' },
        probes: { helper: { ...probe, value: { version: '1.2.4' } }, status: { ...probe, value: { files: 12, semantic_readiness: { state: 'ready', completed_artifacts: 12, selected_artifacts: 12 } } },
            schedule: { ...probe }, vectors: { ...probe }, extraction: { ...probe }, storage: { ...probe },
            integrity: { ...probe, value: { checked_files: 12, total_files: 12, complete: true, from_start: true, skipped_large: 0, failed_objects: 0 } } },
        assessment: { state: 'ready', summary: 'Current indexing is complete.', recommendedAction: 'none' }, recentEvents: []
    };
}

test('diagnostics messages accept only exact fixed actions without command or collection overrides', () => {
    for (const action of ['refresh', 'verify', 'copy', 'retry', 'resume', 'rebuild', 'showOutput'] as const) {
        assert.equal(diagnosticsAction({ type: 'diagnostics', action }), action);
    }
    for (const message of [null, [], {}, { type: 'diagnostics' }, { action: 'retry' },
        { type: 'diagnostics', action: 'delete' }, { type: 'diagnostics', action: ['retry'] },
        { type: 'diagnostics', action: 'retry', command: 'anything' },
        { type: 'diagnostics', action: 'rebuild', collection: 'another' },
        Object.create({ type: 'diagnostics', action: 'rebuild' })]) {
        assert.equal(diagnosticsAction(message), undefined);
    }
});

test('diagnostics escape observed values and keep current readiness separate from earlier errors', () => {
    const report = fixture(), hostile = '</summary><img src=x onerror="alert(1)"><script>alert(2)</script>';
    report.binding.root = hostile;
    report.probes.storage.value = { [hostile]: hostile };
    report.recentEvents = [{ at: report.capturedAt, kind: 'index_failed', helperPath: hostile, error: { kind: 'error', message: hostile } }];
    report.historyError = { kind: 'error', message: hostile };
    const html = renderDiagnosticsReport(hostile, report);
    assert.match(html, /<h2>Ready<\/h2>/);
    assert.match(html, /Current indexing is complete/);
    assert.ok(html.indexOf('Current observations') < html.indexOf('Previous attempts'));
    assert.match(html, /Previous attempt history is unavailable: &lt;\/summary&gt;/);
    assert.match(html, /Current diagnostics and repair actions remain available/);
    assert.match(html, /&lt;img src=x onerror=&quot;alert\(1\)&quot;&gt;/);
    assert.doesNotMatch(html, /<img|<script>alert|command:/);
    assert.match(html, /default-src 'none'/);
    const nonce = html.match(/script-src 'nonce-([a-f0-9]+)'/)?.[1];
    assert.ok(nonce);
    assert.equal((html.match(new RegExp(`nonce="${nonce}"`, 'g')) ?? []).length, 2);
    assert.match(html, /Source files are preserved/);
    assert.match(html, /data-action="rebuild">Rebuild index/);
    report.assessment = { state: 'building', summary: 'Semantic work continues.', recommendedAction: 'wait' };
    const active = renderDiagnosticsReport('Fixture', report);
    assert.match(active, /data-action="rebuild">/);
    assert.match(active, /data-action="retry">/);
    assert.match(active, /data-action="refresh">/);
    report.editor = { paused: false, busy: true, state: 'enriching', pending: true };
    assert.match(renderDiagnosticsReport('Fixture', report), /data-action="verify" disabled/);
    report.assessment.recommendedAction = 'retry';
    assert.match(renderDiagnosticsReport('Fixture', report), /data-action="retry">/,
        'A pending settings change must permit its recommended retry');
});

test('agent configuration distinguishes a saved helper path from current native health and verified connections', () => {
    const report = fixture(), hostile = '</summary><script>unsafe()</script>';
    report.agentConfiguration = { checkedAt: report.capturedAt, state: 'partial', scope: 'known_project_files_and_codex_user',
        filesChecked: 12, filesPresent: 1, truncated: true, connection: 'not_verified',
        entries: [{ client: 'codex', scope: 'user', configPath: hostile, serverName: hostile, helperPath: hostile,
            helperVersion: '1.2.3', status: 'different_helper', issue: hostile }],
        issues: [{ client: 'claude', scope: 'project', configPath: hostile, reason: hostile }] };
    const html = renderDiagnosticsReport('Fixture', report);
    assert.match(html, /<h2>Ready<\/h2>/, 'Saved configuration differences must not rewrite the current native assessment');
    assert.match(html, /Agent configuration/);
    assert.match(html, /Different helper path/);
    assert.match(html, /Current extension helper/);
    assert.match(html, /Version hint from path/);
    assert.match(html, /do not verify an agent connection or an already running helper/);
    assert.match(html, /Connect Coding Agent/);
    assert.match(html, /Some configuration entries were omitted/);
    assert.match(html, /&lt;\/summary&gt;&lt;script&gt;unsafe\(\)&lt;\/script&gt;/);
    assert.doesNotMatch(html, /<script>unsafe|command:/);
});

test('diagnostics panel rejects stale bindings and concurrent repairs and copies only its captured report', async () => {
    let receive: (value: unknown) => Promise<void> = async () => undefined;
    let dispose = () => undefined, valid = true, reads = 0, listenerDisposed = false;
    let pendingRepair: (() => void) | undefined;
    const repairs: string[] = [], copied: string[] = [], report = fixture();
    const webview = { html: '', onDidReceiveMessage: (listener: typeof receive) => {
        receive = listener; return { dispose: () => { listenerDisposed = true; } };
    } };
    const vscode = { ViewColumn: { Active: 1 }, workspace: { isTrusted: true },
        env: { clipboard: { writeText: async (value: string) => { copied.push(value); } } },
        window: { createWebviewPanel: (_type: string, _title: string, _column: unknown, options: unknown) => {
            assert.deepEqual(options, { enableScripts: true, localResourceRoots: [] });
            return { webview, onDidDispose: (callback: typeof dispose) => { dispose = callback; } };
        } } };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? vscode : original.call(this, request, ...args); };
    try {
        await openDiagnosticsPanel({ name: 'Fixture', valid: () => valid,
            diagnose: async () => { reads++; return report; },
            act: async action => { repairs.push(action); if (action === 'retry') { await new Promise<void>(resolve => { pendingRepair = resolve; }); } }
        });
        await receive({ type: 'diagnostics', action: 'copy' });
        assert.deepEqual(JSON.parse(copied[0]), report);
        await receive({ type: 'diagnostics', action: 'retry', root: '/another-workspace' });
        assert.deepEqual(repairs, []);
        report.assessment = { state: 'building', summary: 'A policy change awaits indexing.', recommendedAction: 'retry' };
        const first = receive({ type: 'diagnostics', action: 'retry' });
        await receive({ type: 'diagnostics', action: 'rebuild' });
        assert.deepEqual(repairs, ['retry']);
        valid = false; pendingRepair!(); await first;
        assert.equal(reads, 1, 'A changed binding must not refresh or repair a different collection');
        assert.match(webview.html, /Reopen Diagnose &amp; Repair/);
        await receive({ type: 'diagnostics', action: 'resume' });
        assert.deepEqual(repairs, ['retry']);
        valid = true; vscode.workspace.isTrusted = false;
        await receive({ type: 'diagnostics', action: 'rebuild' });
        assert.deepEqual(repairs, ['retry']);
        dispose(); assert.equal(listenerDisposed, true);
        await receive({ type: 'diagnostics', action: 'refresh' });
        assert.equal(reads, 1);
    } finally { loader._load = original; }
});

test('failed diagnosis remains visible with a working refresh action', async () => {
    let receive: (value: unknown) => Promise<void> = async () => undefined, attempts = 0;
    const webview = { html: '', onDidReceiveMessage: (listener: typeof receive) => { receive = listener; return { dispose() {} }; } };
    const vscode = { ViewColumn: { Active: 1 }, workspace: { isTrusted: true },
        window: { createWebviewPanel: () => ({ webview, onDidDispose() {} }) } };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? vscode : original.call(this, request, ...args); };
    try {
        await openDiagnosticsPanel({ name: 'Fixture', valid: () => true,
            diagnose: async () => { if (++attempts === 1) { throw new Error('Helper is temporarily unavailable'); } return fixture(); }, act: async () => undefined });
        assert.match(webview.html, /role="alert">Helper is temporarily unavailable/);
        assert.match(webview.html, /data-action="refresh">/);
        assert.match(webview.html, /data-action="rebuild" disabled/);
        await receive({ type: 'diagnostics', action: 'refresh' });
        assert.match(webview.html, /Current indexing is complete/);
        assert.doesNotMatch(webview.html, /role="alert"/);
    } finally { loader._load = original; }
});

test('retained verification exposes partial results and stops paging on cancellation, changed storage or lost trust', async () => {
    for (const stop of ['cancel', 'storage', 'trust']) {
        let receive: (value: unknown) => Promise<void> = async () => undefined;
        let valid = true, cancelled = false, pages = 0;
        const partial = fixture();
        partial.assessment = { state: 'unknown', summary: 'Only part of the retained data has been verified.', recommendedAction: 'review' };
        partial.probes.integrity.value = { checked_files: 1, total_files: 12, complete: false, from_start: true, skipped_large: 0, failed_objects: 0 };
        const webview = { html: '', onDidReceiveMessage: (listener: typeof receive) => { receive = listener; return { dispose() {} }; } };
        const token = { get isCancellationRequested() { return cancelled; } };
        const vscode = { ViewColumn: { Active: 1 }, ProgressLocation: { Notification: 1 }, workspace: { isTrusted: true },
            window: { createWebviewPanel: () => ({ webview, onDidDispose() {} }),
                withProgress: async (options: { cancellable: boolean }, run: (progress: unknown, cancellation: unknown) => Promise<unknown>) => {
                    assert.equal(options.cancellable, true);
                    return run({ report: () => { if (stop === 'cancel') { cancelled = true; }
                        else if (stop === 'storage') { valid = false; } else { vscode.workspace.isTrusted = false; } } }, token);
                } } };
        const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
        loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? vscode : original.call(this, request, ...args); };
        try {
            await openDiagnosticsPanel({ name: 'Fixture', valid: () => valid, diagnose: async () => partial,
                act: async () => { assert.fail('Verification must not invoke a repair'); },
                verify: async (report, onProgress, shouldContinue) => {
                    for (let page = 0; page < 3; page++) {
                        if (!shouldContinue()) { break; }
                        pages++; onProgress(report);
                    }
                    return report;
                } });
            assert.match(webview.html, /<h2>Unknown<\/h2>/);
            assert.doesNotMatch(webview.html, /<h2>Ready<\/h2>/);
            await receive({ type: 'diagnostics', action: 'verify' });
            assert.equal(pages, 1, `${stop} must prevent another page request`);
            assert.doesNotMatch(webview.html, /<h2>Ready<\/h2>/);
            if (stop !== 'cancel') { assert.match(webview.html, /Reopen Diagnose &amp; Repair/); }
        } finally { loader._load = original; }
    }
});

test('retained verification refuses active indexing and rechecks after progress setup, page reads and completion', async () => {
    for (const phase of ['initial', 'progress', 'page', 'completion'] as const) {
        let receive: (value: unknown) => Promise<void> = async () => undefined;
        let active = phase === 'initial', calls = 0, pages = 0;
        const observed = fixture();
        observed.assessment = { state: 'unknown', summary: 'Retained data verification is incomplete.', recommendedAction: 'review' };
        const webview = { html: '', onDidReceiveMessage: (listener: typeof receive) => { receive = listener; return { dispose() {} }; } };
        const vscode = { ViewColumn: { Active: 1 }, ProgressLocation: { Notification: 1 }, workspace: { isTrusted: true },
            window: { createWebviewPanel: () => ({ webview, onDidDispose() {} }),
                withProgress: async (_options: unknown, run: (progress: unknown, cancellation: unknown) => Promise<unknown>) => {
                    await Promise.resolve();
                    if (phase === 'progress') { active = true; }
                    return run({ report() {} }, { isCancellationRequested: false });
                } } };
        const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
        loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? vscode : original.call(this, request, ...args); };
        try {
            await openDiagnosticsPanel({ name: 'Fixture', valid: () => true, canVerify: () => !active,
                diagnose: async () => observed, act: async () => { assert.fail('Verification cannot repair or pause indexing'); },
                verify: async (_report, onProgress, shouldContinue) => {
                    calls++;
                    for (let page = 0; page < 3; page++) {
                        if (!shouldContinue()) { break; }
                        pages++; await Promise.resolve();
                        if (phase === 'page') { active = true; }
                        onProgress(observed);
                        if (phase === 'completion') { active = true; break; }
                    }
                    return fixture();
                } });
            if (phase === 'initial') { assert.match(webview.html, /data-action="verify" disabled/); }
            // A forged click or an editor job starting after render must also be rejected by the host.
            await receive({ type: 'diagnostics', action: 'verify' });
            assert.equal(calls, phase === 'initial' || phase === 'progress' ? 0 : 1, phase);
            assert.equal(pages, calls, 'No subsequent retained page is requested after indexing starts');
            assert.match(webview.html, /This collection is indexing/);
            assert.match(webview.html, /data-action="verify" disabled/);
            assert.doesNotMatch(webview.html, /<h2>Ready<\/h2>/, 'A late verified result must not replace observations after new work starts');
            active = false; await receive({ type: 'diagnostics', action: 'refresh' });
            assert.match(webview.html, /data-action="verify">/);
        } finally { loader._load = original; }
    }
});
