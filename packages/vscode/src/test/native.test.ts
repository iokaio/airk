// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { NativeWriteQueue, prepareHelper, probeMcp, runJson } from '../native';
import { IndexReport, LaunchSpec, collectionId } from '../model';
import { Coverage } from '../coverage';

async function removeFixture(directory: string, prefix: string): Promise<void> {
    const resolved = await fs.realpath(directory), base = await fs.realpath(os.tmpdir());
    assert.equal(path.dirname(resolved), base); assert.ok(path.basename(resolved).startsWith(prefix));
    await fs.rm(resolved, { recursive: true, force: true });
}

test('native writer queue isolates data directories and collections', async () => {
    const queue = new NativeWriteQueue();
    const spec: LaunchSpec = { command: '/fixture/airk', root: '/fixture/project', dataDir: '/fixture/state', collection: 'project', name: 'Fixture' };
    let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; });
    const calls: string[] = [];
    const first = queue.run(spec, async () => { calls.push('first'); await held; });
    const same = queue.run({ ...spec, dataDir: path.join(spec.dataDir, '.') }, async () => { calls.push('same'); });
    try {
        await Promise.all([
            queue.run({ ...spec, dataDir: '/fixture/other-storage' }, async () => { calls.push('storage'); }),
            queue.run({ ...spec, collection: 'other-project' }, async () => { calls.push('collection'); })
        ]);
        assert.deepEqual(calls, ['first', 'storage', 'collection']);
    } finally { release(); await Promise.all([first, same]); }
    assert.equal(calls.at(-1), 'same', 'The same native writer must wait even through a path spelling alias');
});

test('bundled helper rejects mismatched platforms and tampered bytes before execution', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-helper-test-'));
    try {
        const native = path.join(directory, 'native'); await fs.mkdir(native);
        const executable = process.platform === 'win32' ? 'airk.exe' : 'airk';
        await fs.writeFile(path.join(native, executable), 'fictional fixture');
        const manifest = { version: '0.1.0', vsix_target: 'wrong-platform', payload: { [executable]: '0'.repeat(64) } };
        await fs.writeFile(path.join(native, 'manifest.json'), JSON.stringify(manifest));
        await assert.rejects(prepareHelper(directory, path.join(directory, 'cache'), '0.1.0'), /does not match/);
        manifest.vsix_target = `${process.platform}-${process.arch}`;
        await fs.writeFile(path.join(native, 'manifest.json'), JSON.stringify(manifest));
        await assert.rejects(prepareHelper(directory, path.join(directory, 'cache'), '0.1.0'), /digest mismatch/);
    } finally { await removeFixture(directory, 'airk-helper-test-'); }
});

// CI/local build script supplies an absolute native binary; ordinary unit runs skip this boundary.
test('extension native adapter indexes, rebuilds, probes MCP, isolates and removes collections', { skip: !process.env.AIRK_TEST_BINARY }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-native-extension-'));
    try {
        const root = path.join(directory, 'Project A'), other = path.join(directory, 'Project B'), dataDir = path.join(directory, 'Index Storage');
        await fs.mkdir(root); await fs.mkdir(other);
        await fs.writeFile(path.join(root, 'a.py'), 'def authenticate_fixture():\n    return True\n');
        await fs.writeFile(path.join(other, 'b.py'), 'def second_fixture():\n    return False\n');
        const command = path.resolve(process.env.AIRK_TEST_BINARY!);
        const spec: LaunchSpec = { command, root, dataDir, collection: collectionId(root), name: 'A' };
        const first = await runJson<IndexReport>(spec, 'index', ['--scheduled']); assert.equal(first.indexed, 1);
        const coverage = await runJson<Coverage>(spec, 'coverage', ['--refresh']); assert.equal(coverage.percent, 100); assert.equal(coverage.current, 1);
        await runJson(spec, 'schedule', ['pause']);
        await assert.rejects(runJson(spec, 'index', ['--scheduled', '--automatic']), /scheduling is paused/);
        assert.equal((await runJson<IndexReport>(spec, 'index', ['--scheduled'])).reused, 1);
        await runJson(spec, 'schedule', ['resume']);
        assert.equal((await runJson<IndexReport>(spec, 'index')).reused, 1);
        const full = await runJson<IndexReport>(spec, 'index', ['--full']); assert.equal(full.reused, 0); assert.ok(full.generation > first.generation);
        const probe = await probeMcp(spec); assert.equal(probe.tools.length, 14);
        assert.ok(probe.tools.includes('airk_code_facts')); assert.ok(probe.tools.includes('airk_relationships'));
        const second = { ...spec, root: other, collection: collectionId(other), name: 'B' };
        await runJson(second, 'index');
        const facts = await runJson<{ facts: unknown[] }>(second, 'symbol', ['authenticate_fixture']); assert.equal(facts.facts.length, 0);
        await runJson(spec, 'collections', ['remove', second.collection]);
        const usage = await runJson<{ report: { calls: number } }>(spec, 'usage', ['report']); assert.ok(usage.report.calls > 0);
        assert.ok((await fs.readFile(path.join(other, 'b.py'), 'utf8')).includes('second_fixture'));
        // Cache validates immutable helper bytes and offers a stable path outside the extension.
        const extension = path.join(directory, 'extension'), native = path.join(extension, 'native'); await fs.mkdir(native, { recursive: true });
        const executable = process.platform === 'win32' ? 'airk.exe' : 'airk', binary = await fs.readFile(command);
        await fs.writeFile(path.join(native, executable), binary);
        await fs.writeFile(path.join(native, 'manifest.json'), JSON.stringify({ version: '0.1.0', vsix_target: `${process.platform}-${process.arch}`,
            payload: { [executable]: createHash('sha256').update(binary).digest('hex') } }));
        const cached = await prepareHelper(extension, path.join(directory, 'cache'), '0.1.0');
        assert.ok(cached.startsWith(path.join(directory, 'cache')));
        assert.equal(await prepareHelper(extension, path.join(directory, 'cache'), '0.1.0'), cached);
    } finally { await removeFixture(directory, 'airk-native-extension-'); }
});
