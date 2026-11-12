// SPDX-License-Identifier: Apache-2.0
// The removal-only recognizer (plan §6.4, §8.4; U12, U92) against the shared
// cases the Rust `airkctl agents` implementation is tested with
// (fixtures/split/agents/config_cases.json): recognition of project entries
// and every reviewed `remove` expectation, byte for byte.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { declarations, recognize, remove, removeAll, type Agent, type Scope } from '../airkctl/agents/recognizer';
import { connectionUrl, inputCredential, reportedConnections, sweepWithoutService, type SweepFileSystem } from '../airkctl/agents';

const repo = path.resolve(__dirname, '../../../..');
interface ConfigCase {
    name: string; agent: Agent; scope: Scope; file: string; source: string | null;
    observations: { verifiedLaunchers: string[] };
    entries?: { containerPath: string[]; serverKey: string; status: string | null }[];
    remove?: { containerPath: string[]; serverKey: string; expect: { text?: string; unchanged?: boolean; refused?: boolean } };
}
const fixture = JSON.parse(fs.readFileSync(path.join(repo, 'fixtures/split/agents/config_cases.json'), 'utf8')) as { schema: string; cases: ConfigCase[] };

test('config cases fixture is the reviewed schema with removal cases', () => {
    assert.equal(fixture.schema, 'airk.split.agents.config_cases/1');
    assert.ok(fixture.cases.filter(item => item.remove).length >= 10, 'zero or missing removal cases is a failure');
});

for (const item of fixture.cases.filter(entry => entry.remove)) {
    test(`remove: ${item.name}`, () => {
        const target = { containerPath: item.remove!.containerPath, serverKey: item.remove!.serverKey };
        const expect = item.remove!.expect;
        if (expect.refused) { assert.throws(() => remove(item.agent, item.scope, item.source!, target)); return; }
        const result = remove(item.agent, item.scope, item.source!, target);
        assert.equal(result, expect.unchanged ? item.source : expect.text);
    });
}

test('project recognition equals the reviewed entries of every readable project case', () => {
    let compared = 0;
    for (const item of fixture.cases) {
        if (item.scope !== 'project' || item.source === null || !item.entries) { continue; }
        let rows;
        try { rows = declarations(item.agent, item.source); } catch { assert.deepEqual(item.entries, [], item.name); continue; }
        const recognized = rows.filter(row => recognize(item.agent, 'project', row, item.observations.verifiedLaunchers))
            .map(row => JSON.stringify([row.container, row.serverKey])).sort();
        const expected = item.entries.map(entry => JSON.stringify([entry.containerPath, entry.serverKey])).sort();
        assert.deepEqual(recognized, expected, item.name); compared++;
    }
    assert.ok(compared >= 10, 'zero or few compared cases is a failure');
});

test('the no-service sweep removes recognized project entries only and keeps every other byte', async () => {
    const claude = fixture.cases.find(item => item.name === 'claude: stdio AIRK entries are retired; an absent launcher is reported')!;
    const foreign = fixture.cases.find(item => item.name === 'claude: pre-split helper and bare token entries are not AIRK-owned')!;
    const files = new Map<string, string>([[path.join('/w', '.mcp.json'), claude.source!], [path.join('/x', '.mcp.json'), foreign.source!]]);
    const fsys: SweepFileSystem = {
        lstat: async file => files.has(file) ? { directory: false, link: false } : [path.resolve('/w'), path.resolve('/x')].includes(path.resolve(file)) ? { directory: true, link: false } : undefined,
        realpath: async file => path.resolve(file),
        read: async file => files.get(file)!,
        write: async (file, text) => { files.set(file, text); },
    };
    const result = await sweepWithoutService(['/w', '/x'], fsys, () => 7);
    assert.deepEqual(result.removed.map(entry => [entry.serverKey, entry.kind, entry.removedAt]).sort(), [['airk', 'stdio', 7], ['airk-project-abc', 'stdio', 7]]);
    assert.equal(files.get(path.join('/w', '.mcp.json')), '{\n  "mcpServers": {}\n}\n');
    assert.equal(files.get(path.join('/x', '.mcp.json')), foreign.source, 'entries AIRK did not provably write are never touched');
    assert.deepEqual(removeAll('claude', foreign.source!).removed, []);
});

test('credential input, connection URL and reported connections are strict', async () => {
    const airkctl = { oneShot: async (argv: string[]) => {
        assert.deepEqual(argv, ['agents', 'credential', '--connection-id', '01'.repeat(16), '--format', 'header-json']);
        return { exitCode: 0, class: 'success', stdout: '{"Authorization":"Bearer abcdefghijklmnopqrstuvwxyz012345"}', stderr: '' };
    } } as never;
    assert.equal(await inputCredential(airkctl, { connection_id: '01'.repeat(16) }), 'abcdefghijklmnopqrstuvwxyz012345');
    await assert.rejects(inputCredential(airkctl, { connection_id: '01'.repeat(16), extra: 1 }), /invalid_http_credential_arguments/);
    await assert.rejects(inputCredential(airkctl, { connection_id: 'NOT-AN-ID' }), /invalid_http_credential_arguments/);
    assert.equal(connectionUrl({ endpoint: { url: 'https://127.0.0.1:57421/mcp' } }), 'https://127.0.0.1:57421/mcp');
    assert.equal(connectionUrl({ endpoint: { url: 'https://example.com/mcp' } }), undefined);
    assert.deepEqual(reportedConnections({ entries: [{ connectionId: '01'.repeat(16) }, { connectionId: '01'.repeat(16) }, { connectionId: 'x' }] }), ['01'.repeat(16)]);
});
