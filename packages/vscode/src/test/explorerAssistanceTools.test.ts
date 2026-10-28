// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { AssistanceTools, toolDefinitions } from '../explorerAssistance/tools';
import { HelpIndex } from '../explorerAssistance/help';
import { windowChannel } from '../explorerAssistance/attach';
import { AssistanceIpcHost } from '../explorerAssistanceHost';
import { ASSISTANCE_TOOLS } from '../explorerContracts';
import { HandoffStore, HANDOFF_TTL_MS } from '../explorerHandoffs';
import { CORE_PACK, jsonBytes } from '../explorerStrategies';
import { SuggestionStore } from '../explorerSuggestions';
import { destinationOf, validateDraft } from '../explorerDrafts';
import { McpStdioSession } from '../mcpStdioSession';

const index = () => new HelpIndex({ catalogVersion: 1, extensionVersion: '1.5.0', topics: [] });
const search = { schema_version: '1.0', idempotency_key: 'dry-run', title: 'Find definitions', draft: { query: 'sample' } };

test('template discovery is offline, paginated by exact bytes, and exposes the actual complete contracts', async () => {
    const tools = new AssistanceTools(index(), async () => { assert.fail('template discovery must not attach'); });
    assert.equal(toolDefinitions(index()).length, 14);
    const ids: string[] = []; let offset = 0;
    do {
        const reply = await tools.call('airk_explorer_templates_list', { offset, limit: 25, max_bytes: 1024 }); assert.ok(reply.ok);
        assert.ok(jsonBytes(reply.value) <= 1024);
        const entries = reply.value.templates as any[]; assert.ok(entries.length);
        ids.push(...entries.map(entry => entry.template_id));
        if (reply.value.next_offset === null) { break; }
        assert.ok(Number(reply.value.next_offset) > offset); offset = Number(reply.value.next_offset);
    } while (offset < CORE_PACK.length);
    assert.deepEqual(ids, CORE_PACK.map(entry => entry.strategy.id));
    for (const entry of CORE_PACK) {
        const reply = await tools.call('airk_explorer_template_get', { template_id: entry.strategy.id }); assert.ok(reply.ok);
        assert.deepEqual(reply.value.strategy, entry.strategy); assert.equal(reply.value.runnable, entry.runnable);
        assert.ok(jsonBytes(reply.value) < 49152);
    }
    assert.equal((await tools.call('airk_explorer_templates_list', { offset: -1 })).ok, false);
    assert.equal((await tools.call('airk_explorer_template_get', { template_id: 'airk.core/unknown' })).ok, false);
});

test('dry-run validation distinguishes offline schema checks from authoritative live conflicts and reserves nothing', async () => {
    const offline = new AssistanceTools(index(), async () => ({ ok: false, code: 'UI_NOT_ATTACHED' }));
    const valid = await offline.call('airk_explorer_validate_draft', { kind: 'search', draft: search }); assert.ok(valid.ok);
    assert.equal(valid.value.valid, true); assert.equal(valid.value.live_checked, false); assert.equal(valid.value.admitted, false);
    const invalid = await offline.call('airk_explorer_validate_draft', { kind: 'search', draft: { ...search, draft: { query: 'x', limit: 0 } } }); assert.ok(invalid.ok);
    assert.equal(invalid.value.valid, false); assert.ok((invalid.value.errors as any[]).some(error => error.field.includes('draft')));
    const context = { binding: 'ab'.repeat(8), revision: 2 };
    const store = new SuggestionStore({ binding: () => context.binding, revisions: () => ({ studio: context.revision }), publication: () => undefined, ttlMs: () => 60000 });
    const tools = new AssistanceTools(index(), async body => {
        assert.equal(body.type, 'validate'); if (body.type !== 'validate') { assert.fail(); }
        return { ok: true, reply: { v: 1, ok: true, validation: store.validate(body.kind, body.draft) } };
    });
    for (let i = 0; i < 40; i++) { const reply = await tools.call('airk_explorer_validate_draft', { kind: 'search', draft: search }); assert.ok(reply.ok && reply.value.live_checked && reply.value.valid); }
    assert.equal(store.list().length, 0);
    assert.ok(store.submit('search', { ...search, draft: { query: 'different body after validation' } }, { name: 'test', version: '1' }).ok, 'validation reserves no idempotency key or rate slot');
    const conflict = await tools.call('airk_explorer_validate_draft', { kind: 'search', draft: search }); assert.ok(conflict.ok); assert.equal(conflict.value.code, 'PROPOSAL_CONFLICT');
    const stale = store.validate('search', { ...search, workspace_binding: 'cd'.repeat(8) }); assert.equal(stale.code, 'STALE_BINDING');
});

test('navigation accepts supported route-specific fields and cannot smuggle execution, paths or filters into another screen', () => {
    const envelope = { schema_version: '1.0', idempotency_key: 'nav', title: 'Inspect this identifier' };
    for (const [route, filters] of [['code/symbols', { name: 'sample', path_prefix: 'src/' }], ['docs/facts', { kind: 'requirement' }], ['evidence/checker', { text: 'src/a.ts:3' }], ['manage/service', {}]] as const) {
        assert.ok(validateDraft('navigation', { ...envelope, route, filters }).ok);
    }
    for (const extra of [{ route: 'manage/service', filters: { name: 'shutdown' } }, { route: 'code/symbols', filters: { path_prefix: '../private' } },
        { route: 'docs/facts', filters: { kind: 'execute' } }, { route: 'evidence/changes', filters: { since: 'unknown' } }, { route: 'code/symbols?run=true' }, { route: 'code/symbols', run: true }]) {
        assert.equal(validateDraft('navigation', { ...envelope, ...extra }).ok, false);
    }
});

test('legacy Management navigation drafts retain their reviewed body and resolve to current tabs', () => {
    for (const [route, destination] of [['manage/projects', 'manage/overview'], ['manage/indexing', 'manage/repair']]) {
        const checked = validateDraft('navigation', { schema_version: '1.0', idempotency_key: 'legacy', title: 'Review indexing', route, filters: {} });
        assert.ok(checked.ok);
        assert.equal(checked.draft.body.kind, 'navigation');
        if (checked.draft.body.kind !== 'navigation') assert.fail();
        assert.equal(checked.draft.body.route, route, 'the stored draft body is preserved');
        assert.equal(destinationOf(checked.draft.body), destination);
        assert.equal(validateDraft('navigation', { schema_version: '1.0', idempotency_key: 'legacy', title: 'Review', route, filters: { run: true } }).ok, false);
    }
});

test('handoff tokens reveal exact shared text only, expire and revoke on revision, binding, trust and evidence changes', () => {
    const state = { authorized: true, binding: 'ab'.repeat(8), revision: 3, publication: 'airk-r1:current', now: 1000 };
    const store = new HandoffStore({ authorized: () => state.authorized, binding: () => state.binding, revisions: () => ({ studio: state.revision }), publication: () => state.publication, now: () => state.now });
    const share = () => store.share('studio', state.revision, 'Complete instructions\n漢字 😀', state.publication);
    const first = share(); assert.equal(store.get(first.token)?.content, first.content);
    assert.equal(first.content_sha256, createHash('sha256').update(first.content).digest('hex'));
    assert.equal(store.get('ex-share-' + '0'.repeat(64)), undefined);
    state.revision++; assert.equal(store.get(first.token), undefined);
    const second = share(); state.publication = 'airk-r1:new'; assert.equal(store.get(second.token), undefined);
    const third = share(); state.binding = 'cd'.repeat(8); assert.equal(store.get(third.token), undefined);
    const fourth = share(); state.authorized = false; assert.equal(store.get(fourth.token), undefined);
    assert.throws(share, /authorization/); state.authorized = true;
    const fifth = share(); state.now += HANDOFF_TTL_MS; assert.equal(store.get(fifth.token), undefined);
    assert.throws(() => store.share('studio', state.revision, '漢'.repeat(9000)), /UTF-8/);
});

test('the real bundled MCP server reaches its project window for validation, navigation, status and shared handoff', { timeout: 30000 }, async t => {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-assistance-tools-')));
    const directory = path.join(root, 'assistance'); let host: AssistanceIpcHost | undefined, session: McpStdioSession | undefined;
    let generation = 1, revision = 2;
    const binding = () => host!.bindingFingerprint;
    const store = new SuggestionStore({ binding, revisions: () => ({ studio: revision }), publication: () => undefined, ttlMs: () => 60000 });
    const handoffs = new HandoffStore({ authorized: () => true, binding, revisions: () => ({ studio: revision }), publication: () => undefined });
    try {
        host = await AssistanceIpcHost.start({ directory, roots: [root], version: '1.5.0', explorerOpen: () => true,
            targetRevisions: () => ({ studio: revision }), uiStatus: () => ({ active_route: 'code/symbols', binding_generation: generation, blockers: [] }),
            validate: (kind, draft) => store.validate(kind, draft), suggest: (kind, draft, client) => store.submit(kind, draft, client),
            proposalStatus: id => store.status(id), handoff: token => handoffs.get(token) }); assert.ok(host);
        session = (await McpStdioSession.open({ command: process.execPath, args: [path.resolve(__dirname, '../explorerAssistance.js'), '--assistance-dir', directory], cwd: root, clientName: 'test', clientVersion: '1' }, 5000)).session;
        const call = async (name: string, args = {}) => (await session!.request('tools/call', { name, arguments: args }, 5000)).structuredContent;
        assert.deepEqual((await session.request('tools/list', {}, 5000)).tools.map((tool: any) => tool.name), [...ASSISTANCE_TOOLS]);
        const status = await call('airk_explorer_ui_status'); assert.equal(status.ui.active_route, 'code/symbols'); assert.equal(status.ui.binding_generation, 1);
        const validation = await call('airk_explorer_validate_draft', { kind: 'search', draft: search }); assert.equal(validation.live_checked, true); assert.equal(store.list().length, 0);
        const proposal = await call('airk_explorer_suggest_navigation', { schema_version: '1.0', title: 'Read references', idempotency_key: 'nav', route: 'code/references', filters: { name: 'sample' } });
        assert.equal(proposal.destination, 'code/references'); assert.equal((await call('airk_explorer_proposal_status', { proposal_id: proposal.proposal_id })).reason, 'awaiting_user_review');
        const shared = handoffs.share('studio', revision, 'Shared complete text');
        assert.equal((await call('airk_explorer_handoff_get', { token: shared.token })).content, 'Shared complete text');
        generation++;
        assert.notEqual((await call('airk_explorer_ui_status')).ui.binding_fingerprint, status.ui.binding_fingerprint);
        assert.equal((await call('airk_explorer_handoff_get', { token: shared.token })).error.code, 'PROPOSAL_EXPIRED');
        const stale = await call('airk_explorer_suggest_search', { ...search, workspace_binding: status.ui.binding_fingerprint }); assert.equal(stale.error.code, 'STALE_BINDING');
        revision++;
    } finally {
        session?.close(); host?.dispose(); await fs.promises.rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
    }
});
