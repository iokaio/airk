// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { ASSISTANCE_TOOLS } from '../explorerContracts';
import { assistanceDirectory, launchDescriptorPath, writeLaunchDescriptor } from '../explorerAssistanceLaunch';
import { AssistanceIpcHost } from '../explorerAssistanceHost';
import { HandoffStore } from '../explorerHandoffs';
import { SuggestionStore } from '../explorerSuggestions';
import { McpStdioSession } from '../mcpStdioSession';
import { assistanceEntry } from '../model';
import { probeAssistance, probeMcp } from '../native';
import { NativeServiceFixture } from './nativeServiceFixture';

// The agent entry runs: stable launcher → runtime `explorer-assistance` forwarder → bundled server.
// The extension root is this package, so the descriptor names the real out/explorerAssistance.js.
const extensionRoot = path.resolve(__dirname, '..', '..');

test('an agent entry reaches Explorer Assistance through the installed launcher and native forwarder', { skip: !process.env.AIRK_TEST_BINARY, timeout: 300000 }, async () => {
    const fixture = await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    const storage = fs.mkdtempSync(path.join(os.tmpdir(), 'airk-assist-native-'));
    let host: AssistanceIpcHost | undefined, session: McpStdioSession | undefined;
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

        const binding = () => host!.bindingFingerprint;
        const inbox = new SuggestionStore({ binding, revisions: () => ({ studio: 1 }), publication: () => undefined, ttlMs: () => 60000 });
        const shares = new HandoffStore({ authorized: () => true, binding, revisions: () => ({ studio: 1 }), publication: () => undefined });
        host = await AssistanceIpcHost.start({ directory: assistanceDirectory(storage), version, roots: [spec.root], explorerOpen: () => true,
            targetRevisions: () => ({ studio: 1 }), uiStatus: () => ({ active_route: 'strategies/studio', binding_generation: 1, blockers: [] }),
            validate: (kind, draft) => inbox.validate(kind, draft), suggest: (kind, draft, client) => inbox.submit(kind, draft, client),
            proposalStatus: id => inbox.status(id), handoff: token => shares.get(token) }); assert.ok(host);
        session = (await McpStdioSession.open({ command: entry.command, args: [...entry.args], cwd: spec.root, clientName: 'native-test', clientVersion: version })).session;
        const call = async (name: string, args = {}) => {
            const result = await session!.request('tools/call', { name, arguments: args }); assert.ok(!result.isError, JSON.stringify(result)); return result.structuredContent;
        };
        assert.equal((await call('airk_explorer_ui_status')).ui.active_route, 'strategies/studio');
        const templates = await call('airk_explorer_templates_list'); assert.ok(templates.templates.length > 0);
        assert.ok((await call('airk_explorer_template_get', { template_id: templates.templates[0].template_id })).strategy.params);
        assert.equal((await call('airk_explorer_validate_draft', { kind: 'search', draft: { schema_version: '1.0', idempotency_key: 'native', title: 'Native check', draft: { query: 'fixture' } } })).live_checked, true);
        const proposal = await call('airk_explorer_suggest_navigation', { schema_version: '1.0', idempotency_key: 'native-nav', title: 'Native navigation', route: 'code/references', filters: { name: 'fixture' } });
        assert.equal((await call('airk_explorer_proposal_status', { proposal_id: proposal.proposal_id })).reason, 'awaiting_user_review');
        const shared = shares.share('studio', 1, 'Complete native shared snapshot');
        assert.equal((await call('airk_explorer_handoff_get', { token: shared.token })).content, shared.content);

        const descriptor = JSON.parse(fs.readFileSync(launchDescriptorPath(storage), 'utf8'));
        fs.writeFileSync(launchDescriptorPath(storage), JSON.stringify({ ...descriptor, server_sha256: '0'.repeat(64) }));
        await assert.rejects(probeAssistance(entry, spec.root, 'native-test'), 'a descriptor whose digest does not match never starts the server');
    } finally {
        session?.close(); host?.dispose();
        try { await fs.promises.rm(storage, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 }); }
        finally { await fixture.dispose(); }
    }
});
