// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { EvidenceClient, EvidenceFailure, EXPLORER_CLIENT_NAME, NativeTool } from '../explorerEvidence';
import { McpSessionError } from '../mcpStdioSession';
import { LaunchSpec } from '../model';

const project = 'ab'.repeat(32);
const spec: LaunchSpec = { command: '/install/airk-launcher', root: '/source', dataDir: '/data', collection: 'demo', name: 'demo',
    service: { installDir: '/install', project } };
const text = (value: unknown) => ({ content: [{ type: 'text', text: JSON.stringify(value) }] });

type Handler = (tool: string, args: any, timeout: number) => Promise<any>;
function harness(handler: Handler, options: { maxInFlight?: number } = {}) {
    const opened: any[] = [];
    let session: any;
    const client = new EvidenceClient(spec, '1.5.0-test', { ...options, retryBudget: 2000, open: async (launch: any) => {
        opened.push(launch);
        session = { closed: false, close() { this.closed = true; },
            request: (method: string, params: any, timeout: number) => { assert.equal(method, 'tools/call'); return handler(params.name, params.arguments, timeout); } };
        return { session };
    } });
    return { client, opened, session: () => session };
}
const failure = (code: string) => (error: unknown) => error instanceof EvidenceFailure && error.error.code === code;

test('the session identifies as the Explorer and returns parsed payloads with publication tokens', async () => {
    const { client, opened } = harness(async tool => tool === 'airk_status'
        ? text({ index_state: 'available', publication: { token: 'airk-r1:abc', generation: 3 } })
        : text({ hits: [], publication: 'airk-r1:def' }));
    const status = await client.call('airk_status', {});
    assert.equal(status.publication, 'airk-r1:abc');
    assert.equal((await client.call('airk_search', { query: 'x' })).publication, 'airk-r1:def');
    assert.equal(opened.length, 1, 'one persistent session');
    assert.equal(opened[0].clientName, EXPLORER_CLIENT_NAME);
    assert.equal(opened[0].clientName, 'airk-explorer');
    assert.deepEqual(opened[0].args.slice(-2), [project, 'mcp']);
    assert.equal(client.project, project);
});

test('native references retain their nested publication and conflicting identities fail closed', async () => {
    const { client } = harness(async (_tool, args) => text({ evidence: { hits: [], publication: { token: 'airk-r1:references' } },
        ...(args.name === 'conflict' ? { publication: { token: 'airk-r1:other' } } : {}) }));
    try {
        assert.equal((await client.call('airk_references', { name: 'sample' })).publication, 'airk-r1:references');
        await assert.rejects(client.call('airk_references', { name: 'conflict' }), failure('EVIDENCE_MISMATCH'));
    } finally { client.dispose(); }
});

test('the evidence queue refuses overload without growing unbounded or losing accepted calls', async () => {
    let release!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const { client } = harness(async () => { await gate; return text({ ok: true }); });
    const pending = Array.from({ length: 67 }, () => client.call('airk_status', {}));
    const done = Promise.all(pending);
    await assert.rejects(client.call('airk_status', {}), failure('RATE_LIMITED'));
    release(); assert.equal((await done).length, 67); client.dispose();
});

test('only explicit native retry hints repeat the same read; tool errors are classified', async () => {
    let attempts = 0;
    const { client } = harness(async tool => {
        if (tool === 'airk_search') {
            attempts++;
            return attempts === 1 ? { isError: true, content: [{ type: 'text', text: JSON.stringify({ status: 'retry', retryable: true, action: 'repeat_same_request', code: 'publication_busy', retry_after_ms: 5 }) }] }
                : text({ hits: [1] });
        }
        if (tool === 'airk_outline') { return { isError: true, content: [{ type: 'text', text: 'workspace is not indexed' }] }; }
        return { isError: true, content: [{ type: 'text', text: JSON.stringify({ error: { code: 'invalid_argument', message: 'path must be relative' } }) }] };
    });
    assert.deepEqual((await client.call('airk_search', { query: 'x' })).payload, { hits: [1] });
    assert.equal(attempts, 2);
    await assert.rejects(client.call('airk_outline', { path: 'a' }), failure('INDEX_UNAVAILABLE'));
    await assert.rejects(client.call('airk_read_file', { path: '/abs' }), (error: unknown) => failure('invalid_argument')(error) && /path must be relative/.test((error as Error).message));
    await assert.rejects(client.call('shell_exec' as NativeTool, {}), failure('INVALID_MESSAGE'));
});

test('at most three calls are outstanding and newer searches supersede older ones', async () => {
    let active = 0, peak = 0;
    const releases: (() => void)[] = [];
    const { client } = harness((tool, args) => new Promise(resolve => {
        active++; peak = Math.max(peak, active);
        releases.push(() => { active--; resolve(text({ tool, n: args.n })); });
    }));
    const plain = [1, 2, 3, 4].map(n => client.call('airk_file_info', { n }));
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(peak, 3);
    const older = assert.rejects(client.call('airk_search', { n: 'old' }, { supersede: 'search' }), failure('REQUEST_SUPERSEDED'),
        'a queued search is dropped when a newer one arrives');
    const newer = client.call('airk_search', { n: 'new' }, { supersede: 'search' });
    await older;
    while (releases.length) { releases.shift()!(); await new Promise(resolve => setImmediate(resolve)); }
    await Promise.all(plain);
    assert.equal((await newer).payload.n, 'new');
    assert.ok(peak <= 3);

    // A superseded search that already started is discarded when it finishes late.
    const started = assert.rejects(client.call('airk_search', { n: 'started' }, { supersede: 'search' }), failure('REQUEST_SUPERSEDED'));
    await new Promise(resolve => setImmediate(resolve));
    const latest = client.call('airk_search', { n: 'latest' }, { supersede: 'search' });
    await new Promise(resolve => setImmediate(resolve));
    while (releases.length) { releases.shift()!(); await new Promise(resolve => setImmediate(resolve)); }
    await started;
    assert.equal((await latest).payload.n, 'latest');
});

test('timeouts and closed sessions map to Explorer errors, and a closed session is reopened', async () => {
    const { client, opened, session } = harness(async tool => {
        if (tool === 'airk_context') { throw new McpSessionError('timeout', 'slow'); }
        if (tool === 'airk_symbols') { session().closed = true; throw new McpSessionError('exited', 'AIRK MCP exited (1)'); }
        return text({ ok: true });
    });
    await assert.rejects(client.call('airk_context', {}), failure('REQUEST_TIMEOUT'));
    await assert.rejects(client.call('airk_symbols', {}), failure('SERVICE_UNAVAILABLE'));
    assert.deepEqual((await client.call('airk_status', {})).payload, { ok: true });
    assert.equal(opened.length, 2);
    client.dispose();
    await assert.rejects(client.call('airk_status', {}), failure('SERVICE_UNAVAILABLE'));
});

test('a project without a native service binding cannot open an evidence session', () => {
    assert.throws(() => new EvidenceClient({ ...spec, service: undefined }, 'x'), failure('NO_PROJECT'));
});
