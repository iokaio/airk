// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { test } from 'node:test';
import { EvidenceClient } from '../explorerEvidence';
import { liveCheck, sourceFile } from '../explorerSource';
import { probeMcp } from '../native';
import { record } from '../serviceClient';
import { NativeServiceFixture } from './nativeServiceFixture';

test('the Explorer evidence session reads one pinned publication through the verified launcher', { skip: !process.env.AIRK_TEST_BINARY, timeout: 300000 }, async () => {
    const fixture = await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    let client: EvidenceClient | undefined;
    try {
        const spec = await fixture.register('explorer');
        await fs.writeFile(path.join(spec.root, 'session.ts'), 'export function refreshSession(token: string) { return token.length > 0; }\n');
        await fixture.configure(spec, { operation: 'vectors', enabled: false });
        await fixture.job(spec);
        const published = await fixture.publishedStatus(spec);
        const token = String(record(published.publication).token);
        assert.equal((await probeMcp(spec)).tools.length, 16, 'the native retrieval tool inventory is unchanged');

        client = new EvidenceClient(spec, 'native-test', { timeout: 60000 });
        const status = await client.call('airk_status', {}, { timeout: 60000 });
        assert.equal(status.payload.index_state, 'available');
        assert.equal(status.publication, token);
        const search = await client.call('airk_search', { query: 'refreshSession', mode: 'lexical', limit: 5 }, { timeout: 60000 });
        assert.equal(search.publication, token, 'search and status share one publication');
        const hits = search.payload.hits as Record<string, any>[];
        assert.ok(Array.isArray(hits) && hits.length === 1);
        assert.equal(hits[0].citation.path, 'session.ts');
        assert.match(String(hits[0].citation.content_hash), /^[0-9a-f]{64}$/);
        const passage = await client.call('airk_read_file', { path: 'session.ts', start_line: 1, end_line: 1 }, { timeout: 60000 });
        assert.equal(passage.publication, token);
        await assert.rejects(client.call('airk_read_file', { path: '../outside.ts', start_line: 1, end_line: 1 }, { timeout: 60000 }));
        // The live check reproduces AIRK's content_hash from the saved bytes at AIRK's own source location.
        const info = (await client.call('airk_file_info', { path: 'session.ts' }, { timeout: 60000 })).payload;
        const source = info.source as Record<string, string>;
        const file = await sourceFile(source.root, source.relative_path);
        assert.equal(await liveCheck(file, String(hits[0].citation.content_hash), false), 'matches_index');
        await fs.appendFile(file, '// edited\n');
        assert.equal(await liveCheck(file, String(hits[0].citation.content_hash), false), 'changed_since_index');
    } finally {
        client?.dispose();
        await fixture.dispose();
    }
});
