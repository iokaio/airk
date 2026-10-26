// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as path from 'node:path';
import { clients, collectionId, configFor, eligibleEvent, launchArgs, LaunchSpec } from '../model';

test('canonical root identities isolate collections and normalize Windows case', () => {
    assert.notEqual(collectionId('/projects/a', 'linux'), collectionId('/projects/b', 'linux'));
    assert.equal(collectionId('D:\\Projects\\A', 'win32'), collectionId('d:\\projects\\a', 'win32'));
    assert.match(collectionId('/projects/a'), /^[a-z0-9-]{1,64}$/);
});
test('workspace escapes, state, secrets and generated files cannot schedule indexing', () => {
    const root = path.resolve('fixture'), data = path.join(root, 'indexes');
    for (const relative of ['../other/file', '.git/index', 'node_modules/package/a.js', '.env.local', 'secret.key', 'indexes/collections/a/snapshot.json']) {
        assert.equal(eligibleEvent(path.resolve(root, relative), root, data), false, relative);
    }
    assert.equal(eligibleEvent(path.join(root, 'src', 'a.ts'), root, data), true);
    assert.equal(eligibleEvent(path.join(root, '.gitignore'), root, data), true);
});
test('all agent fragments preserve separate absolute paths and unique collection server names', () => {
    const spec: LaunchSpec = { command: 'C:\\Program Files\\AIRK\\airk.exe', root: 'D:\\Project A', dataDir: 'E:\\Index Files', collection: 'project-a', name: 'A' };
    for (const client of clients) {
        const text = configFor(client, spec);
        if (client === 'codex') { assert.ok(text.includes('[mcp_servers.airk-project-a]')); assert.ok(text.includes(JSON.stringify(launchArgs(spec)))); }
        else {
            const object = JSON.parse(text);
            const entry = object.servers?.['airk-project-a'] || object.mcpServers?.['airk-project-a'] || object.mcp?.servers?.['airk-project-a'] || object.mcp?.['airk-project-a'] || (object.transport === 'stdio' ? object : undefined);
            const args = Array.isArray(entry.command) ? entry.command.slice(1) : entry.args;
            assert.deepEqual(args, launchArgs(spec));
            assert.equal(Array.isArray(entry.command) ? entry.command[0] : entry.command, spec.command);
        }
    }
});

test('service agent entries use the stable launcher and opaque binding without a worker endpoint',()=>{
    const spec:LaunchSpec={command:path.resolve('installed','airk-launcher'),root:path.resolve('source'),dataDir:path.resolve('data'),
        collection:'fixture',name:'Fixture',service:{installDir:path.resolve('installed'),project:'ab'.repeat(32)}};
    const args=launchArgs(spec);
    assert.deepEqual(args,['runtime','launch','--install-dir',spec.service!.installDir,'--','--root',spec.root,'--data-dir',spec.dataDir,
        '--collection','fixture','--service-project','ab'.repeat(32),'mcp']);
    for(const client of clients)assert.ok(configFor(client,spec).includes('runtime'));
    assert.throws(()=>launchArgs({...spec,service:{...spec.service!,project:'forged'}}),/Invalid trusted service/);
});
