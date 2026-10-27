// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { LiveCheck, parseWebviewMessage } from '../explorerContracts';
import { EvidenceClient, EvidenceFailure, ToolResult } from '../explorerEvidence';
import { FactHandles } from '../explorerFacts';
import { RefRegistry } from '../explorerSearch';
import { CORE_PACK, Strategy } from '../explorerStrategies';
import { runStrategy } from '../explorerStrategyRunner';

const PROJECT = 'ab'.repeat(32);
const HASH = (path: string) => (path.length.toString(16).padStart(2, '0')).repeat(32);
const cite = (path: string, start: number, end = start) => ({ path, start_line: start, end_line: end, content_hash: HASH(path) });
const template = (id: string) => CORE_PACK.find(entry => entry.strategy.id === `airk.core/${id}`)!.strategy;

/** A scripted evidence session; `publication` may change per call to simulate a republish. */
function fake(options: { publication?: (tool: string, call: number) => string } = {}) {
    const calls: { tool: string; args: Record<string, unknown> }[] = [];
    const payloads: Record<string, (args: Record<string, unknown>) => Record<string, unknown>> = {
        airk_status: () => ({ index_state: 'available', version: '1.5.0' }),
        airk_search: () => ({ hits: [{ citation: cite('src/session.ts', 40, 42), text: 'export function renew(token) {}', truncated: false },
            { citation: cite('src/auth/login.ts', 7), text: 'renew(currentToken)', truncated: false }] }),
        airk_symbols: () => ({ items: [{ kind: 'function', name: 'refresh', citation: cite('src/session.ts', 12, 14), symbol_id: 'syntax:refresh', basis: 'syntax' }] }),
        airk_references: () => ({ evidence: { hits: [{ citation: cite('src/auth/login.ts', 7), text: 'renew(currentToken)' }] } }),
        airk_read_file: args => ({ hits: [{ citation: cite(String(args.path), Number(args.start_line), Number(args.end_line)), text: `lines of ${args.path}`, truncated: false }] }),
        airk_file_info: args => ({ source: { root: '/workspace', relative_path: String(args.path) } }),
    };
    const session = {
        call: async (tool: string, args: Record<string, unknown>): Promise<ToolResult> => {
            calls.push({ tool, args });
            const payload = payloads[tool];
            if (!payload) { throw new EvidenceFailure({ code: 'TOOL_ERROR', message: `unexpected ${tool}` }); }
            return { payload: payload(args), publication: options.publication?.(tool, calls.length) ?? 'airk-r1:p:1' };
        },
    } as unknown as EvidenceClient;
    return { session, calls };
}
function deps(session: EvidenceClient, live: (relative: string) => LiveCheck = () => 'matches_index') {
    return { session, facts: new FactHandles(new RefRegistry()), project: PROJECT, extensionVersion: '1.5.0', live: async (_root: string, relative: string) => live(relative), now: () => 1000 };
}

test('a runnable template pins one publication and is a verified snapshot only when every cited file matches', async () => {
    const { session, calls } = fake();
    const result = await runStrategy({ strategy: template('bug'), params: { query: 'renew session', symbol: 'refresh' }, scope: { pathPrefix: 'src/' } }, deps(session));
    assert.equal(result.manifest.outcome, 'verified_snapshot', result.manifest.reasons.join('; '));
    assert.equal(result.manifest.publication, 'airk-r1:p:1');
    assert.equal(result.manifest.serviceVersion, '1.5.0');
    assert.equal(result.manifest.template, 'airk.core/bug@1.0.0');
    assert.match(result.manifest.evidenceDigest, /^[0-9a-f]{64}$/);
    assert.deepEqual(result.records.map(record => `${record.path}:${record.startLine}`), [...result.records].map(record => `${record.path}:${record.startLine}`).sort(), 'records are in total order');
    const search = calls.find(call => call.tool === 'airk_search')!;
    assert.deepEqual({ mode: search.args.mode, path_prefix: search.args.path_prefix, query: search.args.query }, { mode: 'lexical', path_prefix: 'src/', query: 'renew session' });
    assert.ok(calls.every(call => call.tool !== 'airk_help'), 'help is never evidence');
    assert.deepEqual(result.manifest.steps.map(step => step.outcome), result.manifest.steps.map(() => 'ok'));

    const again = await runStrategy({ strategy: template('bug'), params: { query: 'renew session', symbol: 'refresh' }, scope: { pathPrefix: 'src/' } }, deps(fake().session));
    assert.equal(again.manifest.evidenceDigest, result.manifest.evidenceDigest, 'the same evidence gives the same digest');
});

test('a republish during the run fails closed: later steps are skipped and the run is partial', async () => {
    const { session } = fake({ publication: (tool, call) => call >= 3 ? 'airk-r1:p:2' : 'airk-r1:p:1' });
    const result = await runStrategy({ strategy: template('bug'), params: { query: 'renew', symbol: 'refresh' }, scope: {} }, deps(session));
    assert.equal(result.manifest.outcome, 'partial');
    assert.ok(result.manifest.reasons.some(reason => /republished/.test(reason)));
    assert.ok(result.manifest.steps.some(step => step.outcome === 'skipped'));
    assert.equal(result.manifest.publication, 'airk-r1:p:1');
});

test('live mismatches, budgets and unchecked files make the run partial with reasons', async () => {
    const mismatch = await runStrategy({ strategy: template('bug'), params: { query: 'renew', symbol: 'refresh' }, scope: {} },
        deps(fake().session, relative => relative === 'src/session.ts' ? 'changed_since_index' : 'matches_index'));
    assert.equal(mismatch.manifest.outcome, 'partial');
    assert.ok(mismatch.manifest.reasons.some(reason => reason === 'live check: src/session.ts is changed since index'));

    const tight: Strategy = { ...template('bug'), budgets: { ...template('bug').budgets, calls: 3 } };
    const budget = await runStrategy({ strategy: tight, params: { query: 'renew', symbol: 'refresh' }, scope: {} }, deps(fake().session));
    assert.ok(budget.manifest.reasons.some(reason => /call budget of 3/.test(reason)));

    const noLive: Strategy = { ...template('bug'), steps: template('bug').steps.filter(step => step.op !== 'host.hashSavedFiles') };
    const unchecked = await runStrategy({ strategy: noLive, params: { query: 'renew', symbol: 'refresh' }, scope: {} }, deps(fake().session));
    assert.ok(unchecked.manifest.reasons.some(reason => /live files not checked/.test(reason)));
});

test('Preview templates, missing parameters and unsafe scopes are refused before any call', async () => {
    const { session, calls } = fake();
    await assert.rejects(runStrategy({ strategy: template('review'), params: {}, scope: {} }, deps(session)), /Git changes/);
    await assert.rejects(runStrategy({ strategy: template('bug'), params: { query: 'renew' }, scope: {} }, deps(session)), /Symbol or identifier is required/);
    await assert.rejects(runStrategy({ strategy: template('bug'), params: { query: 'a', symbol: 'b', extra: 'c' }, scope: {} }, deps(session)), /Unknown parameter/);
    await assert.rejects(runStrategy({ strategy: template('bug'), params: { query: 'a', symbol: 'b' }, scope: { pathPrefix: '../outside' } }, deps(session)), /relative to the project/);
    assert.equal(calls.length, 0);
});

test('JSON message fields accept bounded plain objects only', () => {
    const base = { v: 1, type: 'strategy.run', requestId: 'r1', params: {}, scope: {} };
    assert.ok(parseWebviewMessage({ ...base, strategy: { a: 1 } }));
    assert.equal(parseWebviewMessage({ ...base, strategy: [1] }), undefined);
    assert.equal(parseWebviewMessage({ ...base, strategy: 'text' }), undefined);
    assert.equal(parseWebviewMessage({ ...base, strategy: { big: 'x'.repeat(40000) } }), undefined);
    assert.equal(parseWebviewMessage({ ...base, strategy: Object.create({ inherited: true }) }), undefined);
});
