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
import { McpStdioSession,McpSessionError } from '../mcpStdioSession';
import { assistanceEntry, unifiedEntry, UNIFIED_TOOLS } from '../model';
import { probeAssistance, probeMcp } from '../native';
import { NativeServiceFixture,NativeTestFailure } from './nativeServiceFixture';
import { requestWithReadRetry } from './nativeReadRetry';
import { record } from '../serviceClient';

// The agent entry runs: stable launcher → runtime `explorer-assistance` forwarder → bundled server.
// The extension root is this package, so the descriptor names the real out/explorerAssistance.js.
const extensionRoot = path.resolve(__dirname, '..', '..');

test('one installed endpoint serves evidence and reviewed UI drafts with independent availability and fixed project scope', { skip: !process.env.AIRK_TEST_BINARY, timeout: 300000 }, async () => {
    const fixture = await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    const storage = fs.mkdtempSync(path.join(os.tmpdir(), 'airk-assist-native-'));
    let host: AssistanceIpcHost | undefined, session: McpStdioSession | undefined, evidence: McpStdioSession | undefined;
    let failure:NativeTestFailure|undefined,lastRead:unknown;
    let phase='assistance setup';
    try {
        await fixture.requireResidentProjects(2);
        const catalogSpec = await fixture.register('assistance');
        // Root sessions qualify offline discovery and UI/runtime metadata only.
        // Fresh catalog evidence always uses its exact reviewed grant below.
        const spec = {...catalogSpec,catalogSession:undefined};
        const version = JSON.parse(fs.readFileSync(path.join(extensionRoot, 'package.json'), 'utf8')).version;
        // Node stands in for the editor's runtime here; the forwarder treats both the same apart from ELECTRON_RUN_AS_NODE.
        await writeLaunchDescriptor(storage, extensionRoot, version, { path: process.execPath, electron: false });
        const legacy = assistanceEntry(spec, launchDescriptorPath(storage))!;
        const entry = unifiedEntry(spec, launchDescriptorPath(storage));
        const catalogEntry = unifiedEntry(catalogSpec,launchDescriptorPath(storage));
        const result = await probeAssistance(legacy, spec.root, 'native-test');
        assert.deepEqual(result.tools, [...ASSISTANCE_TOOLS]);
        assert.equal(result.version, version);
        assert.deepEqual(new Set((await probeMcp(spec,UNIFIED_TOOLS.length,version,entry)).tools),new Set(UNIFIED_TOOLS));
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
        phase='explicit Stop';
        await fixture.run(['runtime','lifecycle','--install-dir',fixture.install],180000,
            JSON.stringify({caller:key.caller,key:key.key,action:'stop',expected_instance:health.instance,runtime:health.runtime_sha256}));
        phase='catalog entry refusal after Stop';
        await assert.rejects(McpStdioSession.open({command:catalogEntry.command,args:[...catalogEntry.args],
            cwd:catalogSpec.root,clientName:'native-test',clientVersion:version}),error=>{
            assert.ok(error instanceof McpSessionError);assert.equal(error.code,'exited');
            assert.match(error.message,/service_explicitly_stopped/);return true;
        });
        const stoppedInspection=await fixture.run(['runtime','inspect','--install-dir',fixture.install]);
        assert.equal(record(stoppedInspection.lifecycle).explicitly_stopped,true);
        assert.equal(record(stoppedInspection.lifecycle).pending,null);
        assert.equal(record(stoppedInspection.selection).active,health.runtime_sha256);
        phase='open unified endpoint after Stop';
        session = (await McpStdioSession.open({ command: entry.command, args: [...entry.args], cwd: spec.root, clientName: 'native-test', clientVersion: version })).session;
        const call = async (name: string, args = {}) => {
            phase=`tool ${name}`;
            const result = await requestWithReadRetry(session!, name, args,120000,reply=>{lastRead={name,reply};}); return result.structuredContent ?? JSON.parse(result.content[0].text);
        };
        assert.equal((await session.request('tools/list',{})).tools.length,UNIFIED_TOOLS.length,'catalog discovery respects explicit Stop');
        assert.ok((await call('help',{topic:'jobs'})).content.includes('job'));
        assert.ok((await call('help',{query:'repair'})).results.length>0);
        const sourceScope=(await call('projects')).default_project;
        const stopped=await call('diagnostics');assert.equal(stopped.index_state,'unavailable');
        assert.equal(stopped.index_error.code,'service_explicitly_stopped');
        assert.equal(stopped.explorer.ui.attached,true,'native Stop does not hide the attached editor');
        await fixture.run(['runtime','start','--install-dir',fixture.install]);
        assert.equal((await call('diagnostics')).explorer.ui.active_route, 'strategies/studio');
        const body=(reply:any)=>reply.structuredContent??JSON.parse(reply.content[0].text);
        const assertRootReadDenied=(reply:any)=>{
            assert.equal(reply.isError,true);
            assert.deepEqual(body(reply).error,{schema:1,code:'scoped_read_unavailable',retryable:false});
        };
        // Metadata/root discovery is not a substitute for a fresh catalog grant.
        // native read: expect the exact permanent bare-root refusal once; never replay it.
        assertRootReadDenied(await session.request('tools/call',{name:'search',arguments:{query:'cobaltmarker',mode:'lexical'}}));
        phase='open explicit catalog evidence and UI endpoint';
        evidence=(await McpStdioSession.open({command:catalogEntry.command,args:[...catalogEntry.args],
            cwd:catalogSpec.root,clientName:'native-test',clientVersion:version})).session;
        const evidenceCall=async(name:string,args={})=>{
            phase=`catalog tool ${name}`;
            return body(await requestWithReadRetry(evidence!,name,args,120000,reply=>{lastRead={name,reply};},catalogSpec.catalogSession));
        };
        const assertGrantScope=(value:any,selected:typeof catalogSpec)=>{
            const scope=value.scope_observation,grant=selected.catalogSession!;
            assert.equal(scope.schema,1);assert.equal(scope.qualifier,'selected_subset');
            assert.equal(scope.provenance,'user_reviewed_project_grant');
            assert.deepEqual(scope.grant,{id:grant.grant_id,revision:grant.expected_grant_revision});
            assert.deepEqual(scope.members.map((member:any)=>member.index_id),[grant.index_id]);
            assert.deepEqual(scope.members.flatMap((member:any)=>member.memberships.map((membership:any)=>membership.project_id)).sort(),grant.project_ids);
        };
        // native read: projects is frontend metadata, with no native evidence query.
        const catalogProjects=body(await evidence.request('tools/call',{name:'projects',arguments:{}}));
        assert.equal(catalogProjects.default_project,catalogSpec.service!.project);
        const firstEvidence=await evidenceCall('search',{query:'cobaltmarker',mode:'lexical',response_mode:'full'});
        assert.ok(firstEvidence.hits.length>0);assertGrantScope(firstEvidence,catalogSpec);
        assert.ok((await evidenceCall('help',{query:'repair'})).results.length>0);
        assert.ok((await evidenceCall('help',{topic:'jobs'})).content.includes('job'));
        const templates = await evidenceCall('list_templates'); assert.ok(templates.templates.length > 0);
        assert.ok((await evidenceCall('get_template', { template_id: templates.templates[0].template_id })).strategy.params);
        assert.equal((await evidenceCall('validate_draft', { kind: 'search', draft: { schema_version: '1.0', idempotency_key: 'native', title: 'Native check', draft: { query: 'fixture' } } })).live_checked, true);
        const proposal = await evidenceCall('suggest_view', {kind:'navigation',draft:{ schema_version: '1.0', idempotency_key: 'native-nav', title: 'Native navigation', route: 'code/references', filters: { name: 'fixture' } }});
        assert.equal((await evidenceCall('proposal_status', { proposal_id: proposal.proposal_id })).reason, 'awaiting_user_review');
        const shared = shares.share('studio', 1, 'Complete native shared snapshot');
        assert.equal((await evidenceCall('get_handoff', { token: shared.token })).content, shared.content);

        selected='cd'.repeat(32);
        // native read: a deliberate negative test must receive exactly STALE_BINDING once.
        const wrong=await evidence.request('tools/call',{name:'draft_search',arguments:{schema_version:'1.0',idempotency_key:'wrong-project',title:'Must reject',draft:{query:'x'}}});
        assert.equal(wrong.isError,true);assert.equal(wrong.structuredContent.error.code,'STALE_BINDING');
        assert.equal((await call('status')).project,sourceScope,'UI selection never switches the indexed project');
        const fixedEvidence=await evidenceCall('status');assertGrantScope(fixedEvidence,catalogSpec);
        assert.deepEqual(fixedEvidence.scope_observation,firstEvidence.scope_observation,'UI selection never changes the reviewed evidence authority');
        host.dispose();host=undefined;
        assert.ok((await evidenceCall('search',{query:'cobaltmarker',mode:'lexical'})).hits.length>0,'editor closure does not block retrieval');
        assert.equal((await call('diagnostics')).explorer.ui.attached,false);
        const descriptor = JSON.parse(fs.readFileSync(launchDescriptorPath(storage), 'utf8'));
        fs.writeFileSync(launchDescriptorPath(storage), JSON.stringify({ ...descriptor, server_sha256: '0'.repeat(64) }));
        await assert.rejects(probeAssistance(legacy, spec.root, 'native-test'), 'a descriptor whose digest does not match never starts the server');
        assert.equal((await call('diagnostics')).explorer.error.code,'UI_UNAVAILABLE');
        assert.ok((await evidenceCall('airk_search',{query:'cobaltmarker',mode:'lexical'})).hits.length>0,'hidden native alias remains callable');
        assert.equal(inbox.list().length,1,'only the approved navigation proposal reached the Inbox');

        const catalogOther=await fixture.register('other');
        const other={...catalogOther,catalogSession:undefined};
        assert.throws(()=>unifiedEntry(catalogSpec,undefined,[catalogOther]),/Catalog sessions support one explicit native selection/);
        fs.writeFileSync(path.join(other.root,'other.txt'),'berylmarker isolated evidence');
        await fixture.configure(other,{operation:'vectors',enabled:false});await fixture.job(other);
        const multiple=unifiedEntry(spec,undefined,[other]);
        const multi=(await McpStdioSession.open({command:multiple.command,args:[...multiple.args],cwd:spec.root,clientName:'native-test',clientVersion:version})).session;
        try{
            // native read: projects is answered by the frontend scope table without a native query.
            const scopes=await multi.request('tools/call',{name:'projects',arguments:{}});
            assert.equal(scopes.structuredContent.projects.length,2);assert.equal(scopes.structuredContent.default_project,null);
            // native read: a deliberate negative test must receive exactly PROJECT_REQUIRED once.
            const ambiguous=await multi.request('tools/call',{name:'search',arguments:{query:'cobaltmarker',mode:'lexical'}});
            assert.equal(ambiguous.isError,true);assert.equal(ambiguous.structuredContent.error.code,'PROJECT_REQUIRED');
            for(const [project,query] of [[scopes.structuredContent.projects.find((p:any)=>p.name===spec.root).project,'cobaltmarker'],[scopes.structuredContent.projects.find((p:any)=>p.name===other.root).project,'berylmarker']]){
                // native read: each explicit bare-root binding must refuse exactly once.
                assertRootReadDenied(await multi.request('tools/call',{name:'search',arguments:{project,query,mode:'lexical'}}));
            }
        }finally{multi.close();}
        // Each positive source read retains the actual native grant created for
        // that source. Multi-root discovery above grants neither source access.
        for(const [selectedSpec,query,file,foreignQuery,foreignFile] of [
            [catalogSpec,'cobaltmarker','fixture.txt','berylmarker','other.txt'],
            [catalogOther,'berylmarker','other.txt','cobaltmarker','fixture.txt'],
        ] as const){
            const selectedEntry=unifiedEntry(selectedSpec);
            const selectedSession=(await McpStdioSession.open({command:selectedEntry.command,args:[...selectedEntry.args],
                cwd:selectedSpec.root,clientName:'native-test',clientVersion:version})).session;
            try{
                phase=`selected evidence ${selectedSpec.name}`;
                const result=await requestWithReadRetry(selectedSession,'search',{query,mode:'lexical',response_mode:'full'},120000,reply=>{lastRead={query,reply};},selectedSpec.catalogSession);
                const value=body(result);assert.ok(JSON.stringify(result).includes(file));
                assert.ok(!JSON.stringify(result).includes(foreignFile));assertGrantScope(value,selectedSpec);
                const foreign=body(await requestWithReadRetry(selectedSession,'search',{query:foreignQuery,mode:'lexical',response_mode:'full'},120000,reply=>{lastRead={query:foreignQuery,reply};},selectedSpec.catalogSession));
                assert.deepEqual(foreign.hits,[]);assertGrantScope(foreign,selectedSpec);
            }finally{selectedSession.close();}
        }
    } catch(error) { failure={error,context:{phase,lastRead}}; }
    finally {
        const errors:unknown[]=failure?[failure.error]:[];
        try{session?.close();}catch(error){errors.push(error);}
        try{evidence?.close();}catch(error){errors.push(error);}
        try{host?.dispose();}catch(error){errors.push(error);}
        try{await fs.promises.rm(storage,{recursive:true,force:true,maxRetries:20,retryDelay:100});}
        catch(error){errors.push(error);}
        const error=errors.length>1?new AggregateError(errors,'Assistance body and local cleanup failed'):errors[0];
        await fixture.dispose(errors.length?{error,context:failure?.context}:undefined,[session,evidence].filter((value):value is McpStdioSession=>value!==undefined));
    }
});
