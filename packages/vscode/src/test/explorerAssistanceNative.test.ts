// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { stdioLaunch } from './stdioLaunch';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { test } from 'node:test';
import { ASSISTANCE_TOOLS } from '../explorerContracts';
import { assistanceDirectory, launchDescriptorPath, writeLaunchDescriptor } from '../explorerAssistanceLaunch';
import { AssistanceIpcHost } from '../explorerAssistanceHost';
import { HandoffStore } from '../explorerHandoffs';
import { SuggestionStore } from '../explorerSuggestions';
import { McpStdioSession } from '../mcpStdioSession';
import { UNIFIED_TOOLS } from '../model';
import { probeAssistance } from '../native';
import { NativeServiceFixture,NativeTestFailure } from './nativeServiceFixture';
import { requestWithReadRetry } from './nativeReadRetry';
import { record } from '../serviceClient';
import {NativeHttpSession} from './nativeHttpSession';
import {InstalledHttpFailure} from './installedEditorHttp';

// The agent entry runs: stable launcher → runtime `explorer-assistance` forwarder → bundled server.
// The extension root is this package, so the descriptor names the real out/explorerAssistance.js.
const extensionRoot = path.resolve(__dirname, '..', '..');

test('one installed endpoint serves evidence and reviewed UI drafts with independent availability and fixed project scope', { skip: !process.env.AIRK_TEST_BINARY, timeout: 300000 }, async () => {
    const fixture = await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    let host: AssistanceIpcHost | undefined, session: McpStdioSession | undefined, evidence: NativeHttpSession | undefined;
    let failure:NativeTestFailure|undefined,lastRead:unknown;
    let phase='assistance setup';
    try {
        const storage = fs.mkdtempSync(path.join(fixture.owner, 'airk-assist-native-'));
        await fixture.requireResidentProjects(2);
        const catalogSpec = await fixture.register('assistance');
        // Offline Node Assistance has no evidence authority. The service HTTPS
        // endpoint uses only the exact reviewed native grant below.
        const spec=catalogSpec;
        const version = JSON.parse(fs.readFileSync(path.join(extensionRoot, 'package.json'), 'utf8')).version;
        // Node stands in for the editor's runtime here; the forwarder treats both the same apart from ELECTRON_RUN_AS_NODE.
        await writeLaunchDescriptor(storage, extensionRoot, version, { path: process.execPath, electron: false });
        const legacy={key:'airk_explorer',label:'Native Assistance',command:fixture.binary,args:['explorer-assistance','--descriptor',launchDescriptorPath(storage)]};
        const result = await probeAssistance(legacy, spec.root, 'native-test', stdioLaunch);
        assert.deepEqual(result.tools, [...ASSISTANCE_TOOLS]);
        assert.equal(result.version, version);
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
        phase='HTTP evidence refusal after exact Stop';
        await assert.rejects(NativeHttpSession.open(catalogSpec.http!,'stopped-evidence-test',10000),error=>error instanceof InstalledHttpFailure&&error.kind==='network'&&error.detail.response_bytes===0);
        const stoppedInspection=await fixture.run(['runtime','inspect','--install-dir',fixture.install]);
        assert.equal(record(stoppedInspection.lifecycle).explicitly_stopped,true);
        assert.equal(record(stoppedInspection.lifecycle).pending,null);
        assert.equal(record(stoppedInspection.selection).active,health.runtime_sha256);
        phase='open standalone Node Assistance after Stop';
        session=(await McpStdioSession.open({command:legacy.command,args:legacy.args,cwd:spec.root,clientName:'native-test',clientVersion:version,launch:stdioLaunch})).session;
        const assistance=async(name:string,args={})=>{
            // native read: one offline Node Assistance request proves UI/help availability while evidence is stopped.
            const result=await session!.request('tools/call',{name,arguments:args});assert.notEqual(result.isError,true,JSON.stringify(result));return result.structuredContent??JSON.parse(result.content[0].text);
        };
        assert.equal((await session.request('tools/list',{})).tools.length,ASSISTANCE_TOOLS.length,'Standalone Assistance remains separate from stopped evidence transport');
        assert.equal((await assistance('airk_explorer_ui_status')).ui.attached,true,'Native Stop does not hide attached editor');
        assert.ok((await assistance('airk_explorer_help_search',{query:'repair'})).results.length>0);
        await fixture.run(['runtime','start','--install-dir',fixture.install]);
        assert.equal((await assistance('airk_explorer_ui_status')).ui.active_route,'strategies/studio');
        const body=(reply:any)=>reply.structuredContent??JSON.parse(reply.content[0].text);
        // A bare source declaration is not imported authority and cannot enroll.
        await fixture.refuseUnimportedLegacy(catalogSpec);
        phase='open explicit HTTPS catalog evidence and attached UI endpoint';
        const windowDescriptor=host.descriptorPath;assert.ok(windowDescriptor);
        assert.equal(path.dirname(windowDescriptor),path.join(assistanceDirectory(storage),'windows'));
        assert.match(path.basename(windowDescriptor),/^[a-f0-9]{32}\.json$/);
        assert.notEqual(windowDescriptor,launchDescriptorPath(storage),'Enrollment attaches this exact window, not global launch metadata');
        evidence=await fixture.openHttp(catalogSpec,'native-assistance-evidence',windowDescriptor);
        assert.equal((await evidence.request('tools/list',{})).tools.length,UNIFIED_TOOLS.length);
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
        const fixedEvidence=await evidenceCall('status');assertGrantScope(fixedEvidence,catalogSpec);
        assert.deepEqual(fixedEvidence.scope_observation,firstEvidence.scope_observation,'UI selection never changes the reviewed evidence authority');
        host.dispose();host=undefined;
        assert.ok((await evidenceCall('search',{query:'cobaltmarker',mode:'lexical'})).hits.length>0,'editor closure does not block retrieval');
        assert.equal((await assistance('airk_explorer_ui_status')).ui.attached,false);
        const descriptor = JSON.parse(fs.readFileSync(launchDescriptorPath(storage), 'utf8'));
        fs.writeFileSync(launchDescriptorPath(storage), JSON.stringify({ ...descriptor, server_sha256: '0'.repeat(64) }));
        await assert.rejects(probeAssistance(legacy, spec.root, 'native-test', stdioLaunch), 'a descriptor whose digest does not match never starts the server');
        assert.equal((await assistance('airk_explorer_ui_status')).ui.attached,false);
        assert.ok((await evidenceCall('airk_search',{query:'cobaltmarker',mode:'lexical'})).hits.length>0,'hidden native alias remains callable');
        assert.equal(inbox.list().length,1,'only the approved navigation proposal reached the Inbox');

        const catalogOther=await fixture.register('other');
        const other=catalogOther;
        fs.writeFileSync(path.join(other.root,'other.txt'),'berylmarker isolated evidence');
        await fixture.configure(other,{operation:'vectors',enabled:false});await fixture.job(other);
        await fixture.refuseUnimportedLegacy(other);
        // Each positive source read retains its exact reviewed native grant.
        // Refused source declarations above grant neither index access.
        for(const [selectedSpec,query,file,foreignQuery,foreignFile] of [
            [catalogSpec,'cobaltmarker','fixture.txt','berylmarker','other.txt'],
            [catalogOther,'berylmarker','other.txt','cobaltmarker','fixture.txt'],
        ] as const){
            const selectedSession=await fixture.openHttp(selectedSpec,'native-isolation-test');
            try{
                phase=`selected evidence ${selectedSpec.name}`;
                const result=await requestWithReadRetry(selectedSession,'search',{query,mode:'lexical',response_mode:'full'},120000,reply=>{lastRead={query,reply};},selectedSpec.catalogSession);
                const value=body(result);assert.ok(JSON.stringify(result).includes(file));
                assert.ok(!JSON.stringify(result).includes(foreignFile));assertGrantScope(value,selectedSpec);
                const foreign=body(await requestWithReadRetry(selectedSession,'search',{query:foreignQuery,mode:'lexical',response_mode:'full'},120000,reply=>{lastRead={query:foreignQuery,reply};},selectedSpec.catalogSession));
                assert.deepEqual(foreign.hits,[]);assertGrantScope(foreign,selectedSpec);
            }finally{await selectedSession.close();}
        }
    } catch(error) { failure={error,context:{phase,lastRead}}; }
    finally {
        const errors:unknown[]=failure?[failure.error]:[];
        try{session?.close();}catch(error){errors.push(error);}
        try{await evidence?.close();}catch(error){errors.push(error);}
        try{host?.dispose();}catch(error){errors.push(error);}
        const error=errors.length>1?new AggregateError(errors,'Assistance body and local cleanup failed'):errors[0];
        await fixture.dispose(errors.length?{error,context:failure?.context}:undefined,[...(session?[session]:[]),...(evidence?[evidence]:[])]);
    }
});
