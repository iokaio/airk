// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { test } from 'node:test';
import { EvidenceClient } from '../explorerEvidence';
import { continueFacts, FactHandles, runFacts } from '../explorerFacts';
import { RefRegistry } from '../explorerSearch';
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
        const help = await client.call('airk_help', { topic: 'search', max_bytes: 8192 }, { timeout: 60000 });
        assert.equal(help.payload.topic, 'search');
        assert.match(String(help.payload.content), /Lexical search/);
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

test('Explorer code and document views read real symbol ids, candidate edges and paged facts', { skip: !process.env.AIRK_TEST_BINARY, timeout: 300000 }, async () => {
    const fixture = await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    let client: EvidenceClient | undefined;
    try {
        const spec = await fixture.register('explorer-code');
        await fs.mkdir(path.join(spec.root, 'src'), { recursive: true });
        await fs.writeFile(path.join(spec.root, 'src', 'token.ts'), 'export function verifyToken(token: string): boolean {\n  return token.length > 3;\n}\n');
        await fs.writeFile(path.join(spec.root, 'src', 'session.ts'), "import { verifyToken } from './token';\n\nexport function refreshSession(token: string) {\n  return verifyToken(token);\n}\n");
        await fs.writeFile(path.join(spec.root, 'requirements.md'), '# Requirements\n\n## REQ-AUTH-7 Session renewal\n\nThe system shall renew sessions before expiry.\n');
        await fixture.configure(spec, { operation: 'vectors', enabled: false });
        await fixture.job(spec);
        await fixture.publishedStatus(spec);
        client = new EvidenceClient(spec, 'native-test', { timeout: 60000 });
        const handles = new FactHandles(new RefRegistry());
        const project = spec.service!.project;
        const facts = await runFacts(client, handles, project, { tool: 'airk_code_facts', limit: 10, args: { name: 'refreshSession', limit: 10 } });
        assert.equal(facts.items.length, 1);
        assert.ok(facts.items[0].symbolRef, 'the native symbol id becomes a handle');
        const symbol = handles.symbols.get(facts.items[0].symbolRef!, project);
        const edges = await runFacts(client, handles, project, { tool: 'airk_relationships', limit: 20, args: { symbol: symbol.symbolId, direction: 'outgoing', limit: 20 } });
        assert.ok(edges.items.some(edge => edge.kind === 'call' && edge.resolution === 'syntax_candidate' && /verifyToken/.test(edge.name)), JSON.stringify(edges.items));
        const references = await runFacts(client, handles, project, { tool: 'airk_references', limit: 10, args: { name: 'verifyToken', limit: 10 } });
        assert.ok(references.items.length >= 2 && references.items.every(item => item.evidenceClass === 'E1'));
        const requirements = await runFacts(client, handles, project, { tool: 'airk_document_facts', limit: 10, args: { kind: 'requirement', limit: 10 } });
        assert.equal(requirements.items[0].path, 'requirements.md');
        const page = await runFacts(client, handles, project, { tool: 'airk_list_files', limit: 1, args: { limit: 1 } });
        assert.ok(page.more, 'list_files returns a continuation cursor');
        const next = await continueFacts(client, handles, project, page.more!);
        assert.equal(next.publication, page.publication);
    } finally {
        client?.dispose();
        await fixture.dispose();
    }
});
