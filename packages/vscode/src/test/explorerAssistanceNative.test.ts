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
import { assistanceEntry, unifiedEntry, UNIFIED_TOOLS } from '../model';
import { probeAssistance, probeMcp } from '../native';
import { NativeServiceFixture } from './nativeServiceFixture';
import { record } from '../serviceClient';

// The agent entry runs: stable launcher → runtime `explorer-assistance` forwarder → bundled server.
// The extension root is this package, so the descriptor names the real out/explorerAssistance.js.
const extensionRoot = path.resolve(__dirname, '..', '..');

test('one installed endpoint serves evidence and reviewed UI drafts with independent availability and fixed project scope', { skip: !process.env.AIRK_TEST_BINARY, timeout: 300000 }, async () => {
    const fixture = await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    const storage = fs.mkdtempSync(path.join(os.tmpdir(), 'airk-assist-native-'));
    let host: AssistanceIpcHost | undefined, session: McpStdioSession | undefined;
    try {
        const spec = await fixture.register('assistance');
        const version = JSON.parse(fs.readFileSync(path.join(extensionRoot, 'package.json'), 'utf8')).version;
        // Node stands in for the editor's runtime here; the forwarder treats both the same apart from ELECTRON_RUN_AS_NODE.
        writeLaunchDescriptor(storage, extensionRoot, version, { path: process.execPath, electron: false });
        const legacy = assistanceEntry(spec, launchDescriptorPath(storage))!;
        const entry = unifiedEntry(spec, launchDescriptorPath(storage));
        const result = await probeAssistance(legacy, spec.root, 'native-test');
        assert.deepEqual(result.tools, [...ASSISTANCE_TOOLS]);
        assert.equal(result.version, version);
        assert.deepEqual(new Set((await probeMcp(spec,27,version,entry)).tools),new Set(UNIFIED_TOOLS));
        fs.writeFileSync(path.join(spec.root,'fixture.txt'),'cobaltmarker native evidence');
        await fixture.configure(spec,{operation:'vectors',enabled:false});await fixture.job(spec);
        let selected=spec.service!.project;

        const binding = () => host!.bindingFingerprint;
        const inbox = new SuggestionStore({ binding, revisions: () => ({ studio: 1 }), publication: () => undefined, ttlMs: () => 60000 });
        const shares = new HandoffStore({ authorized: () => true, binding, revisions: () => ({ studio: 1 }), publication: () => undefined });
        host = await AssistanceIpcHost.start({ directory: assistanceDirectory(storage), version, roots: [spec.root], explorerOpen: () => true,
            targetRevisions: () => ({ studio: 1 }), uiStatus: () => ({ active_route: 'strategies/studio', binding_generation: 1, selected_project:selected, blockers: [] }),
            validate: (kind, draft) => inbox.validate(kind, draft), suggest: (kind, draft, client) => inbox.submit(kind, draft, client),
            proposalStatus: id => inbox.status(id), handoff: token => shares.get(token) }); assert.ok(host);
        const health=record((await fixture.client!.rpc('health','health')).observation);
        const key=await fixture.client!.prepareJob(spec.service!.project,'index');
        await fixture.run(['runtime','lifecycle','--install-dir',fixture.install],180000,
            JSON.stringify({caller:key.caller,key:key.key,action:'stop',expected_instance:health.instance,runtime:health.runtime_sha256}));
        session = (await McpStdioSession.open({ command: entry.command, args: [...entry.args], cwd: spec.root, clientName: 'native-test', clientVersion: version })).session;
        const call = async (name: string, args = {}) => {
            const result = await session!.request('tools/call', { name, arguments: args }); assert.ok(!result.isError, JSON.stringify(result)); return result.structuredContent ?? JSON.parse(result.content[0].text);
        };
        assert.equal((await session.request('tools/list',{})).tools.length,27,'catalog discovery respects explicit Stop');
        assert.ok((await call('help',{topic:'jobs'})).content.includes('job'));
        assert.ok((await call('help',{query:'repair'})).results.length>0);
        const sourceScope=(await call('projects')).default_project;
        const stopped=await call('diagnostics');assert.equal(stopped.index_state,'unavailable');
        assert.equal(stopped.index_error.code,'service_explicitly_stopped');
        assert.equal(stopped.explorer.ui.attached,true,'native Stop does not hide the attached editor');
        await fixture.run(['runtime','start','--install-dir',fixture.install]);
        assert.equal((await call('diagnostics')).explorer.ui.active_route, 'strategies/studio');
        assert.ok((await call('search',{query:'cobaltmarker',mode:'lexical',response_mode:'full'})).hits.length>0);
        assert.ok((await call('help',{query:'repair'})).results.length>0);
        assert.ok((await call('help',{topic:'jobs'})).content.includes('job'));
        const templates = await call('list_templates'); assert.ok(templates.templates.length > 0);
        assert.ok((await call('get_template', { template_id: templates.templates[0].template_id })).strategy.params);
        assert.equal((await call('validate_draft', { kind: 'search', draft: { schema_version: '1.0', idempotency_key: 'native', title: 'Native check', draft: { query: 'fixture' } } })).live_checked, true);
        const proposal = await call('suggest_view', {kind:'navigation',draft:{ schema_version: '1.0', idempotency_key: 'native-nav', title: 'Native navigation', route: 'code/references', filters: { name: 'fixture' } }});
        assert.equal((await call('proposal_status', { proposal_id: proposal.proposal_id })).reason, 'awaiting_user_review');
        const shared = shares.share('studio', 1, 'Complete native shared snapshot');
        assert.equal((await call('get_handoff', { token: shared.token })).content, shared.content);

        selected='cd'.repeat(32);
        const wrong=await session.request('tools/call',{name:'draft_search',arguments:{schema_version:'1.0',idempotency_key:'wrong-project',title:'Must reject',draft:{query:'x'}}});
        assert.equal(wrong.isError,true);assert.equal(wrong.structuredContent.error.code,'STALE_BINDING');
        assert.equal((await call('status')).project,sourceScope,'UI selection never switches the indexed project');
        host.dispose();host=undefined;
        assert.ok((await call('search',{query:'cobaltmarker',mode:'lexical'})).hits.length>0,'editor closure does not block retrieval');
        assert.equal((await call('diagnostics')).explorer.ui.attached,false);
        const descriptor = JSON.parse(fs.readFileSync(launchDescriptorPath(storage), 'utf8'));
        fs.writeFileSync(launchDescriptorPath(storage), JSON.stringify({ ...descriptor, server_sha256: '0'.repeat(64) }));
        await assert.rejects(probeAssistance(legacy, spec.root, 'native-test'), 'a descriptor whose digest does not match never starts the server');
        assert.equal((await call('diagnostics')).explorer.error.code,'UI_UNAVAILABLE');
        assert.ok((await call('airk_search',{query:'cobaltmarker',mode:'lexical'})).hits.length>0,'hidden native alias remains callable');
        assert.equal(inbox.list().length,1,'only the approved navigation proposal reached the Inbox');

        const other=await fixture.register('other');
        fs.writeFileSync(path.join(other.root,'other.txt'),'berylmarker isolated evidence');
        await fixture.configure(other,{operation:'vectors',enabled:false});await fixture.job(other);
        const multiple=unifiedEntry(spec,undefined,[other]);
        const multi=(await McpStdioSession.open({command:multiple.command,args:[...multiple.args],cwd:spec.root,clientName:'native-test',clientVersion:version})).session;
        try{
            const scopes=await multi.request('tools/call',{name:'projects',arguments:{}});
            assert.equal(scopes.structuredContent.projects.length,2);assert.equal(scopes.structuredContent.default_project,null);
            const ambiguous=await multi.request('tools/call',{name:'search',arguments:{query:'cobaltmarker',mode:'lexical'}});
            assert.equal(ambiguous.isError,true);assert.equal(ambiguous.structuredContent.error.code,'PROJECT_REQUIRED');
            for(const [project,query,file] of [[scopes.structuredContent.projects.find((p:any)=>p.name===spec.root).project,'cobaltmarker','fixture.txt'],[scopes.structuredContent.projects.find((p:any)=>p.name===other.root).project,'berylmarker','other.txt']]){
                const result=await multi.request('tools/call',{name:'search',arguments:{project,query,mode:'lexical'}});
                assert.notEqual(result.isError,true);assert.ok(JSON.stringify(result).includes(file));
            }
        }finally{multi.close();}
    } finally {
        session?.close(); host?.dispose();
        try { await fs.promises.rm(storage, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 }); }
        finally { await fixture.dispose(); }
    }
});
