// SPDX-License-Identifier: Apache-2.0
// U105/U107 Workbench and Explorer parity for open index/set selection: the
// exact `airkctl tools call` argv an explicit selection sends through the
// window session, the unchanged default path, input validation, the
// "Max results per index" control, and complete rendering of the result.
// Expected argv, frames and values are written out here, never derived from
// the code under test.
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import { test } from 'node:test';
import { AirkctlToolSession, evidenceSelection, selectionArguments } from '../airkctl/tools';
import { encodeRequest, requestFrameValid } from '../airkctl/session';
import { openSelectionKey, openSelectionSpec } from '../airkctl/features/windowScope';
import { parseWebviewMessage, scopeSpansIndexes } from '../explorerContracts';
import { searchArgs } from '../explorerSearch';
import { limitPerIndexField, object, openSelectionLines, parseWorkbenchMessage, toolFormArguments, validateOpenSelection, workbenchToolFields, type Value, type WorkbenchSession, type WorkbenchState } from '../workbenchContracts';
import { markdown, openSelectionArgv, renderExport } from '../workbenchRender';
import { newSession } from '../workbenchSessions';
import type { LaunchSpec } from '../model';

const idx = (n: number) => 'idx_' + n.toString(16).padStart(2, '0').repeat(16);
const prj = (n: number) => 'prj_' + n.toString(16).padStart(2, '0').repeat(16);
const base = { command: '/install/airkctl', installDir: '/install', root: '/src/app' };
const success = (body: unknown) => ({ exitCode: 0, class: 'success' as const, stdout: JSON.stringify({ content: [{ type: 'text', text: JSON.stringify(body) }], isError: false }), stderr: '' });

test('an explicit selection sends --index/--set through the session invoke; the default paths are unchanged', async () => {
    const seen: string[][] = [];
    const fake = { closed: false, invoke: async (argv: string[], options: { deadlineMs: number }) => { seen.push(argv); assert.equal(options.deadlineMs, 60000); return success({}); } };
    const named = openSelectionSpec(base, { indexes: [idx(1), 'Docs'], sets: [prj(2)] }, 'Docs');
    const window: LaunchSpec = { command: base.command, root: base.root, dataDir: '', collection: 'window-scope', name: 'scope', service: { installDir: base.installDir, project: 'ef'.repeat(32) }, windowScope: { key: 'ef'.repeat(32), label: 'Set 1' } };
    const project: LaunchSpec = { command: base.command, root: base.root, dataDir: '/data', collection: 'app', name: 'app', service: { installDir: base.installDir, project: 'ab'.repeat(32) } };
    for (const [spec, client, windowActive] of [[named, 'airk-workbench', false], [named, 'airk-explorer', true], [window, 'airk-workbench', false], [project, 'airk-workbench', false], [project, 'airk-explorer', true], [project, 'airk-workbench', true]] as const) {
        const tools = new AirkctlToolSession(fake as never, () => evidenceSelection(spec, client, windowActive));
        await tools.request('tools/call', { name: 'search', arguments: { query: 'retry', limit_per_index: 2 } }, 60000);
    }
    const args = '{"query":"retry","limit_per_index":2}';
    assert.deepEqual(seen, [
        ['tools', 'call', 'search', '--args', args, '--index', idx(1), '--index', 'Docs', '--set', prj(2), '--output', 'json'],
        ['tools', 'call', 'search', '--args', args, '--index', idx(1), '--index', 'Docs', '--set', prj(2), '--output', 'json'],
        ['tools', 'call', 'search', '--args', args, '--output', 'json'],
        ['tools', 'call', 'search', '--args', args, '--service-project', 'ab'.repeat(32), '--output', 'json'],
        ['tools', 'call', 'search', '--args', args, '--output', 'json'],
        ['tools', 'call', 'search', '--args', args, '--output', 'json'],
    ], 'explicit names never also name the history key (--service-project), a window grant or a session');
    assert.deepEqual(selectionArguments({ open: { indexes: [], sets: ['Team docs'] }, window: true, project: 'ab'.repeat(32) }), ['--set', 'Team docs']);
    // The session client accepts the frame the child decodes (`SessionRequest`, deny_unknown_fields).
    const id = '0a'.repeat(16), frame = encodeRequest(id, { invoke: { argv: seen[0], deadline_ms: 60000 } } as never);
    const body = JSON.parse(frame.subarray(4).toString('utf8'));
    assert.deepEqual(body, { id, request: { invoke: { argv: seen[0], deadline_ms: 60000 } } });
    assert.equal(requestFrameValid(body), true);
});

test('explicit selections are validated before they are sent', () => {
    assert.deepEqual(validateOpenSelection({ indexes: [' Docs '], sets: [] }), { indexes: ['Docs'], sets: [] });
    assert.deepEqual(openSelectionLines('Docs\r\n\n  ' + prj(1) + '  \n'), ['Docs', prj(1)]);
    for (const [raw, message] of [
        [{ indexes: [], sets: [] }, /at least one/],
        [{ indexes: ['Docs', 'Docs'], sets: [] }, /named twice/],
        [{ indexes: [''], sets: [] }, /empty/],
        [{ indexes: ['--set'], sets: [] }, /starts with "-"/],
        [{ indexes: ['a\u0007b'], sets: [] }, /control character/],
        [{ indexes: ['x'.repeat(257)], sets: [] }, /256 bytes/],
        [{ indexes: Array.from({ length: 33 }, (_, n) => `i${n}`), sets: [] }, /at most 32 indexes/],
        [{ indexes: [], sets: Array.from({ length: 33 }, (_, n) => `s${n}`) }, /at most 32 index sets/],
        [{ indexes: ['a'], sets: [], connection: 'x' }, /only indexes and index sets/],
    ] as const) { assert.throws(() => validateOpenSelection(raw), message, JSON.stringify(raw).slice(0, 60)); }
    assert.ok(parseWorkbenchMessage({ type: 'openSelection', indexes: ['Docs'], sets: [prj(1)] }));
    assert.ok(parseWorkbenchMessage({ type: 'scopeChoices' }));
    for (const invalid of [{ type: 'openSelection', indexes: [' Docs'], sets: [] }, { type: 'openSelection', indexes: [], sets: [] },
        { type: 'openSelection', indexes: ['-x'], sets: [] }, { type: 'openSelection', indexes: ['a'] }, { type: 'openSelection', indexes: ['a'], sets: [], extra: 1 }]) {
        assert.equal(parseWorkbenchMessage(invalid), undefined, JSON.stringify(invalid));
    }
    // The history key ignores order and keeps names exactly as given.
    assert.equal(openSelectionKey({ indexes: ['b', 'a'], sets: [] }), openSelectionKey({ indexes: ['a', 'b'], sets: [] }));
    assert.notEqual(openSelectionKey({ indexes: ['a'], sets: [] }), openSelectionKey({ indexes: [], sets: ['a'] }));
    assert.match(openSelectionKey({ indexes: ['a'], sets: [] }), /^[0-9a-f]{64}$/);
});

test('R02 per-call admission refuses checking and failed automatic routes without sending a fallback read', async () => {
    const project: LaunchSpec = { command: base.command, root: base.root, dataDir: '/data', collection: 'app', name: 'app', service: { installDir: base.installDir, project: 'ab'.repeat(32) } };
    const window: LaunchSpec = { ...project, windowScope: { key: 'ef'.repeat(32), label: 'A' } };
    const named = openSelectionSpec(base, { indexes: [idx(1)], sets: [prj(2)] }, 'Selected');
    const sent: string[][] = [];
    const fake = { closed: false, invoke: async (argv: string[]) => { sent.push(argv); return success({}); } };
    // The route is a per-call observation; accepting an extra optional input
    // lets the old implementation run and reproduce its missing admission check.
    const select: (spec: LaunchSpec, client: 'airk-explorer' | 'airk-workbench', active: boolean, route: { state: 'checking' | 'failed' | 'scope'; automatic?: boolean; message?: string }) => ReturnType<typeof evidenceSelection> = evidenceSelection;
    for (const state of ['checking', 'failed'] as const) {
        const route = { state, message: 'The requested index scope is unavailable.' };
        for (const [spec, client] of [[project, 'airk-explorer'], [project, 'airk-workbench'], [window, 'airk-workbench']] as const) {
            for (const heldA of [true, false]) {
                const tools = new AirkctlToolSession(fake as never, () => select(spec, client, heldA, route));
                await assert.rejects(tools.request('tools/call', { name: 'search', arguments: { query: 'B' } }, 60000), { code: 'unavailable' });
            }
        }
        assert.deepEqual(sent, [], 'neither an old W4 grant nor project binding is sent');
        const explicit = new AirkctlToolSession(fake as never, () => select(named, 'airk-workbench', false, route));
        await explicit.request('tools/call', { name: 'search', arguments: { query: 'selected' } }, 60000);
        assert.deepEqual(sent.splice(0), [['tools', 'call', 'search', '--args', '{"query":"selected"}', '--index', idx(1), '--set', prj(2), '--output', 'json']], 'explicit named selections remain usable');
    }
    const chosen = new AirkctlToolSession(fake as never, () => select(project, 'airk-explorer', true, { state: 'scope', automatic: false }));
    await chosen.request('tools/call', { name: 'search', arguments: { query: 'chosen' } }, 60000);
    assert.deepEqual(sent, [['tools', 'call', 'search', '--args', '{"query":"chosen"}', '--output', 'json']]);
});

const searchSchema = { properties: { query: { type: 'string' }, limit: { type: 'integer', minimum: 1, maximum: 100 }, max_bytes: { type: 'integer' } }, required: ['query'] };
const declared = { limit_per_index: { type: 'integer', minimum: 1, maximum: 100, with: 'limit' } };

test('"Max results per index" follows limit only when the catalog declares combined_controls.limit_per_index', () => {
    assert.deepEqual(workbenchToolFields(searchSchema, declared).map(field => field.name), ['query', 'limit', 'limit_per_index', 'max_bytes']);
    assert.deepEqual(limitPerIndexField(searchSchema, declared), { name: 'limit_per_index', label: 'Max results per index', type: 'integer', required: false, minimum: 1, maximum: 100,
        description: 'Each index returns at most this many results; Limit stays the overall cap.' });
    assert.deepEqual(toolFormArguments(searchSchema, { query: 'retry', limit: '8', limit_per_index: '4' }, declared), { query: 'retry', limit: 8, limit_per_index: 4 });
    for (const value of ['0', '101', '2.5', 'many']) { assert.throws(() => toolFormArguments(searchSchema, { query: 'q', limit_per_index: value }, declared), /Max results per index/); }
    // Single-index catalogs and operations without limit never offer it, so it is never sent.
    assert.deepEqual(workbenchToolFields(searchSchema).map(field => field.name), ['query', 'limit', 'max_bytes']);
    assert.equal(limitPerIndexField({ properties: { path: { type: 'string' } } }, declared), undefined);
    assert.throws(() => toolFormArguments(searchSchema, { query: 'q', limit_per_index: '2' }), /current tool form/);
    assert.deepEqual(toolFormArguments(searchSchema, { query: 'q', limit: '3' }), { query: 'q', limit: 3 }, 'the default form is unchanged');
});

test('Explorer search sends limit_per_index only for a search scope spanning several indexes', () => {
    const run = { v: 1 as const, type: 'search.run' as const, requestId: 'r', query: 'retry', mode: 'lexical' as const, limit: 20, maxBytes: 8192, fullChunks: false };
    assert.deepEqual(searchArgs(run), { query: 'retry', limit: 20, max_bytes: 8192, full_chunks: false, mode: 'lexical' }, 'the default arguments are unchanged');
    assert.deepEqual(searchArgs({ ...run, limitPerIndex: 3 }), { query: 'retry', limit: 20, limit_per_index: 3, max_bytes: 8192, full_chunks: false, mode: 'lexical' });
    assert.ok(parseWebviewMessage({ ...run, limitPerIndex: 1 }));
    assert.ok(parseWebviewMessage({ ...run, limitPerIndex: 100 }));
    for (const value of [0, 101, 2.5, '3']) { assert.equal(parseWebviewMessage({ ...run, limitPerIndex: value }), undefined, String(value)); }
    assert.equal(scopeSpansIndexes({ active: true, indexIds: [idx(1), idx(2)] }), true);
    for (const summary of [{ active: true, indexIds: [idx(1)] }, { active: false, indexIds: [idx(1), idx(2)] }, { active: true }, undefined]) {
        assert.equal(scopeSpansIndexes(summary), false, JSON.stringify(summary));
    }
});

function combinedEnvelope(): Value {
    const native = { publication: { token: 'airk-r1:t' }, hits: [] };
    const members = [1, 2].map(n => ({ index_id: idx(n), index_name: `Docs ${n}`, elapsed_ms: 10 * n, allocation: { limit: 2 }, outcome: 'complete',
        result: { content: [{ type: 'text', text: JSON.stringify(native) }], structuredContent: native, isError: false }, future_member_field: { retained: true } }));
    return { executed: true, status: 'complete', plan: { steps: [{ id: 'search', tool: 'search', args: { query: 'retry', limit_per_index: 2 } }] },
        steps: [{ id: 'search', tool: 'search', status: 'complete', result: { schema: 'airk.combined-read/1', selection: { index_ids: [idx(1), idx(2)] }, members, ranking: [], partial: false, truncated: false } }] };
}

class Emitter<T> { readonly event = (_listener: (event: T) => unknown) => ({ dispose() {} }); fire(_value: T): void {} dispose(): void {} }
function workbench() {
    const specs: LaunchSpec[] = [], calls: [string, Value][] = [], clipboard: string[] = [];
    const project: LaunchSpec = { command: base.command, root: base.root, dataDir: '/data', collection: 'app', name: 'App', service: { installDir: base.installDir, project: 'ab'.repeat(32) } };
    class Client {
        constructor(readonly spec: LaunchSpec) { specs.push(spec); }
        call(tool: string, args: Value): Promise<{ payload: Value }> {
            calls.push([tool, args]);
            if (tool === 'airk_capabilities') { return Promise.resolve({ payload: { workbench: { operations: { search: searchSchema }, combined_controls: declared } } }); }
            if (tool === 'airk_parse') { return Promise.resolve({ payload: combinedEnvelope() }); }
            return Promise.resolve({ payload: { index_state: 'available' } });
        }
        cancel(): void {} dispose(): void {}
    }
    const vscode = { EventEmitter: Emitter, env: { clipboard: { writeText: async (text: string) => { clipboard.push(text); } } },
        workspace: { getConfiguration: () => ({ get: (_key: string, fallback: unknown) => fallback, inspect: () => ({ globalValue: true }) }), onDidChangeConfiguration: () => ({ dispose() {} }) },
        window: {}, commands: { executeCommand: async () => undefined }, Uri: { joinPath: (b: { fsPath: string }, ...parts: string[]) => ({ fsPath: [b.fsPath, ...parts].join('/') }), file: (fsPath: string) => ({ fsPath }) } };
    const loader = Module as unknown as { _load: (name: string, ...args: any[]) => any }, original = loader._load;
    loader._load = function (name: string, ...args: any[]) {
        if (name === 'vscode') { return vscode; }
        if (name === './explorerEvidence') { return { ...original.call(this, name, ...args), EvidenceClient: Client }; }
        return original.call(this, name, ...args);
    };
    let View: typeof import('../workbenchView').WorkbenchView;
    try { delete require.cache[require.resolve('../workbenchView')]; ({ WorkbenchView: View } = require('../workbenchView')); } finally { loader._load = original; }
    const host = { trusted: () => true, specs: () => [project], activeProject: () => 'ab'.repeat(32), onDidChangeSpecs: () => ({ dispose() {} }), version: '1.8.35',
        openSelectionSpec: (selection: { indexes: readonly string[]; sets: readonly string[] }, label: string) => openSelectionSpec(base, selection, label) };
    const provider = new View!({ globalStorageUri: { fsPath: '/unused' }, extensionUri: { fsPath: '/ext' } } as any, host as any);
    type Internal = { session?: WorkbenchSession; sessions: unknown; saveQueue: Promise<void>; state(): WorkbenchState; message(message: unknown): Promise<void> };
    const internal = provider as unknown as Internal;
    internal.sessions = { save: async (session: WorkbenchSession) => { session.revision++; return []; }, list: async () => [], load: async () => { throw new Error('none'); }, remove: async () => undefined, findShortcut: async () => undefined };
    internal.session = newSession('ab'.repeat(32));
    const send = async (value: unknown) => { const parsed = parseWorkbenchMessage(value); assert.ok(parsed, JSON.stringify(value)); await internal.message(parsed); await internal.saveQueue; };
    return { provider, internal, specs, calls, clipboard, send };
}

test('the Workbench reads a named selection as its own entry with complete results, copies and exports', async () => {
    const f = workbench();
    try {
        await f.send({ type: 'openSelection', indexes: ['Docs 1', idx(2)], sets: [prj(3)] });
        const key = openSelectionKey({ indexes: ['Docs 1', idx(2)], sets: [prj(3)] });
        let state = f.internal.state();
        assert.equal(state.project, key);
        assert.deepEqual(state.openSelection, { indexes: ['Docs 1', idx(2)], sets: [prj(3)] });
        assert.deepEqual(state.projects.map(entry => entry.id), ['ab'.repeat(32), key], 'the default entry stays available beside the named selection');
        const spec = f.specs.at(-1)!;
        assert.deepEqual(evidenceSelection(spec, 'airk-workbench', false), { open: { indexes: ['Docs 1', idx(2)], sets: [prj(3)] } });
        await f.send({ type: 'forms' });
        assert.deepEqual(f.internal.state().combinedControls, declared);
        await f.send({ type: 'run', input: '/search retry --limit-per-index 2', prepare: false });
        const parse = f.calls.filter(([tool]) => tool === 'airk_parse');
        assert.deepEqual(parse.map(([, args]) => args), [{ schema: 'airk.parse.request/1', mode: 'execute', text: '/search retry --limit-per-index 2' }], 'the slash operation reaches the native parser unchanged');
        state = f.internal.state();
        const turn = state.session!.turns.at(-1)!;
        assert.deepEqual(turn.selection, { indexes: ['Docs 1', idx(2)], sets: [prj(3)] });
        assert.deepEqual(turn.envelope, combinedEnvelope(), 'the complete native result is retained');
        // Copies and exports keep the whole result, member names, timings and the selection.
        await f.send({ type: 'copy', turn: turn.id, format: 'json' });
        const exported = JSON.parse(f.clipboard.at(-1)!);
        assert.deepEqual(exported.turns[0].envelope, combinedEnvelope());
        assert.deepEqual(exported.turns[0].selection, { indexes: ['Docs 1', idx(2)], sets: [prj(3)] });
        await f.send({ type: 'copy', turn: turn.id, format: 'markdown' });
        const text = f.clipboard.at(-1)!;
        const escaped = (id: string) => id.replace('_', String.fromCharCode(92) + '_');
        assert.ok(text.includes(`Explicit selection: --index Docs 1 --index ${escaped(idx(2))} --set ${escaped(prj(3))}`), 'Markdown escapes the names as it escapes paths');
        for (const fragment of ['"index_name": "Docs 2"', '"elapsed_ms": 20', '"future_member_field"']) { assert.ok(text.includes(fragment), fragment); }
        // The default entry reads under its own selection again, with no named indexes.
        await f.send({ type: 'project', id: 'ab'.repeat(32) });
        assert.equal(f.internal.state().openSelection, undefined);
        assert.deepEqual(evidenceSelection(f.specs.at(-1)!, 'airk-workbench', false), { project: 'ab'.repeat(32) });
    } finally { f.provider.dispose(); }
});

test('exports and Markdown keep the explicit selection beside the complete result', () => {
    const turn = { id: 't', at: 1, input: 'q', format: 'structured' as const, envelope: combinedEnvelope(), selection: { indexes: ['Docs'], sets: [] } };
    const session = { schema: 1 as const, id: 's', project: 'ab'.repeat(32), title: 't', createdAt: 1, updatedAt: 1, revision: 1, omitted: 0, turns: [turn] };
    const whole: any = JSON.parse(renderExport(session, 'json'));
    assert.deepEqual(whole.turns[0], turn);
    assert.deepEqual(openSelectionArgv({ indexes: ['Docs', idx(1)], sets: [prj(1)] }), ['--index', 'Docs', '--index', idx(1), '--set', prj(1)]);
    assert.deepEqual(openSelectionArgv({ indexes: ['Docs', idx(1)], sets: [prj(1)] }), selectionArguments({ open: { indexes: ['Docs', idx(1)], sets: [prj(1)] } }), 'the display and the sent argv agree');
    assert.ok(markdown(turn).includes('Explicit selection: --index Docs'));
    assert.equal(JSON.parse(renderExport(session, 'json')).turns[0].envelope.steps[0].result.members.length, 2, 'both members, with their names and timings, stay in the export');
});
