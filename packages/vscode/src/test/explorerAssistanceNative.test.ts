// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { ASSISTANCE_TOOLS } from '../explorerContracts';
import { launchDescriptorPath, writeLaunchDescriptor } from '../explorerAssistanceLaunch';
import { assistanceEntry } from '../model';
import { probeAssistance, probeMcp } from '../native';
import { NativeServiceFixture } from './nativeServiceFixture';

// The agent entry runs: stable launcher → runtime `explorer-assistance` forwarder → bundled server.
// The extension root is this package, so the descriptor names the real out/explorerAssistance.js.
const extensionRoot = path.resolve(__dirname, '..', '..');

test('an agent entry reaches Explorer Assistance through the installed launcher and native forwarder', { skip: !process.env.AIRK_TEST_BINARY, timeout: 300000 }, async () => {
    const fixture = await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    const storage = fs.mkdtempSync(path.join(os.tmpdir(), 'airk-assist-native-'));
    try {
        const spec = await fixture.register('assistance');
        const version = JSON.parse(fs.readFileSync(path.join(extensionRoot, 'package.json'), 'utf8')).version;
        // Node stands in for the editor's runtime here; the forwarder treats both the same apart from ELECTRON_RUN_AS_NODE.
        writeLaunchDescriptor(storage, extensionRoot, version, { path: process.execPath, electron: false });
        const entry = assistanceEntry(spec, launchDescriptorPath(storage))!;
        const result = await probeAssistance(entry, spec.root, 'native-test');
        assert.deepEqual(result.tools, [...ASSISTANCE_TOOLS]);
        assert.equal(result.version, version);
        assert.equal((await probeMcp(spec)).tools.length, 16, 'AIRK Retrieval still exposes exactly its 16 tools and no Explorer tools');

        const descriptor = JSON.parse(fs.readFileSync(launchDescriptorPath(storage), 'utf8'));
        fs.writeFileSync(launchDescriptorPath(storage), JSON.stringify({ ...descriptor, server_sha256: '0'.repeat(64) }));
        await assert.rejects(probeAssistance(entry, spec.root, 'native-test'), 'a descriptor whose digest does not match never starts the server');
    } finally {
        fs.rmSync(storage, { recursive: true, force: true });
        await fixture.dispose();
    }
});
