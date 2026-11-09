// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import {createHash} from 'node:crypto';
import {globalConfigDeclarations} from '../globalAgentInventory';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { parse } from 'jsonc-parser';
import { LaunchSpec, unifiedEntry, assistanceEntry, serializeServers, RETRIEVAL_TOOLS } from '../model';
import { launchDescriptorPath } from '../explorerAssistanceLaunch';
import { LocalAgent, migrateStandaloneHttpEntry, httpConfigurationConnectionId, refreshHttpAgentConfig, hasUnifiedVscodeConfig, migrateAgentConfig, migrateLocalAgents, captureCleanBreakClientCleanup, applyCleanBreakClientCleanup } from '../agentMigration';
import { httpAuthorityDigest } from '../serviceHttp';
import {predecessorHttpFixture} from './fixtures/predecessorHttpFixture';

const storage=path.resolve('fixture-storage');
const spec:LaunchSpec={root:path.resolve('fixture-source'),dataDir:path.resolve('fresh-data'),collection:'project-fixture',name:'fixture',
    command:path.resolve('installed-runtime','airk-launcher'),service:{installDir:path.resolve('installed-runtime'),project:'ab'.repeat(32)}};
const name='airk-project-fixture';

test('clean break removes expired predecessor HTTP declarations from disposal proof without manufacturing live grants',async()=>{
    const lookup=predecessorHttpFixture(path.join(spec.service!.installDir,'old-control'),'ab'.repeat(32)),{snapshot,...proof}=lookup;
    const environment='AIRK_MCP_TOKEN_'+proof.connection_id.toUpperCase(),source=JSON.stringify({mcpServers:{airk:{type:'http',url:proof.endpoint.url,headers:{Authorization:`Bearer \${${environment}}`},disabled:true},other:{type:'http',url:'https://foreign.invalid/mcp'}}});
    const destination=path.join(spec.root,'.mcp.json');
    const plan=await captureCleanBreakClientCleanup(spec.service!.installDir,storage,[{agent:'claude',destination,source}],[],()=>{},async()=>proof as any);
    assert.deepEqual(plan.blockers,[]);assert.equal(plan.files.length,1);assert.equal(JSON.stringify(plan).includes('"selection"'),false);assert.equal(JSON.stringify(plan).includes('"grant"'),false);
    let current=source;const applied=await applyCleanBreakClientCleanup(plan,spec.service!.installDir,new Set([destination]),()=>{},async()=>current,async(_file,_before,after)=>{current=after;});
    assert.equal(applied.complete,true);assert.deepEqual(parse(current).mcpServers,{other:{type:'http',url:'https://foreign.invalid/mcp'}});
});
const entry={command:path.join(storage,'cli','bin',process.platform==='win32'?'airk.exe':'airk'),
    args:['--root',spec.root,'--data-dir',path.resolve('old-data'),'--collection',spec.collection,'mcp'],env:{AIRK_CLIENT:'codex'},disabled:true};
function json(agent:LocalAgent,value:unknown=entry):string{return `// Keep this comment\n${JSON.stringify({[agent==='copilot'?'servers':'mcpServers']:{[name]:value,other:{command:'other'}},inputs:[]},null,2)}\n`;}
function toml(value=entry):string{return `model = "user-model"\n[mcp_servers.${name}]\ncommand = ${JSON.stringify(value.command)}\nargs = ${JSON.stringify(value.args)}\nenabled = false\n[mcp_servers.${name}.env]\nAIRK_CLIENT = "codex"\n[mcp_servers.other]\ncommand = "other"\n`;}

test('clean break removes exact disabled client declarations for multiple original indexes without replacing authority',async()=>{
    const second:LaunchSpec={...spec,root:path.resolve('second-source'),collection:'second',service:{...spec.service!,project:'cd'.repeat(32)}};
    const server=(saved:LaunchSpec)=>({...unifiedEntry(saved,launchDescriptorPath(storage)),disabled:true});
    const source=JSON.stringify({keep:'foreign-setting',mcpServers:{airk:server(spec),'airk-second':server(second),other:{command:'foreign-command',env:{PRIVATE:'do-not-copy'}}}});
    const destination=path.join(spec.root,'.mcp.json');
    const plan=await captureCleanBreakClientCleanup(spec.service!.installDir,storage,[{agent:'claude',destination,source}],[spec,second],()=>{});
    assert.deepEqual(plan.blockers,[]);assert.equal(plan.files.length,1);assert.equal(plan.files[0].removals.length,2);
    assert.ok(!JSON.stringify(plan).includes('do-not-copy'),'unrelated configuration bytes are not persisted in cleanup plans');
    let current=source,saves=0;
    const apply=()=>applyCleanBreakClientCleanup(plan,spec.service!.installDir,new Set([destination]),()=>{},async()=>current,async(_destination,before,after)=>{assert.equal(before,current);saves++;current=after;});
    assert.equal((await apply()).complete,true);
    assert.deepEqual(parse(current),{keep:'foreign-setting',mcpServers:{other:{command:'foreign-command',env:{PRIVATE:'do-not-copy'}}}});
    assert.equal((await apply()).complete,true);assert.equal(saves,1,'recovery verifies the exact saved removal without replay');
    assert.equal(globalConfigDeclarations('claude',current).some(row=>row.serverKey==='airk'),false);
});

test('clean break preserves unproven AIRK declarations and changed files with explicit blockers',async()=>{
    const destination=path.join(spec.root,'.mcp.json'),source=JSON.stringify({mcpServers:{airk:unifiedEntry(spec,launchDescriptorPath(storage)),airk_custom:{command:'user-owned-server'},other:{command:'foreign'}}});
    const plan=await captureCleanBreakClientCleanup(spec.service!.installDir,storage,[{agent:'claude',destination,source}],[spec],()=>{});
    assert.equal(plan.blockers[0].code,'clean_break_client_ownership_unconfirmed');
    const changed=source+'\n';let saves=0;
    const result=await applyCleanBreakClientCleanup(plan,spec.service!.installDir,new Set([destination]),()=>{},async()=>changed,async()=>{saves++;});
    assert.equal(result.complete,false);assert.equal(saves,0);assert.ok(result.blockers.some(row=>row.code==='clean_break_client_configuration_changed'));
    await assert.rejects(applyCleanBreakClientCleanup(plan,spec.service!.installDir,new Set(),()=>{},async()=>source,async()=>{}),/destination_unconfirmed/);
});

test('clean break rejects modified selectors and launcher names absent from native approval',async()=>{
    const destination=path.join(spec.root,'.mcp.json');
    const original={command:spec.command,args:['--root',spec.root,'--data-dir',spec.dataDir,'--collection',spec.collection,'--service-project',spec.service!.project,'mcp']};
    const capture=async(value:unknown,launchers:readonly string[]=[])=>captureCleanBreakClientCleanup(spec.service!.installDir,storage,[{agent:'claude',destination,source:JSON.stringify({mcpServers:{airk:value}})}],[spec],()=>{},undefined,launchers);
    assert.equal((await capture(original)).files.length,1);
    for(const [flag,value] of [['--data-dir',path.resolve('unproven-storage')],['--service-project','ff'.repeat(32)]] as const){
        const modified=structuredClone(original);modified.args[modified.args.indexOf(flag)+1]=value;
        const result=await capture(modified);assert.equal(result.files.length,0);assert.equal(result.blockers[0].code,'clean_break_client_ownership_unconfirmed');
    }
    const derived=path.join(spec.service!.installDir,process.platform==='win32'?'airk-launcher.exe':'airk-launcher');
    if(derived!==spec.command){
        const changed={...original,command:derived};assert.equal((await capture(changed)).files.length,0);
        assert.equal((await capture(changed,[derived])).files.length,1,'only native-supplied launcher proof enables removal');
    }
    const cached={...original,command:path.join(storage,'cli','bin',process.platform==='win32'?'airk.exe':'airk')};
    assert.equal((await capture(cached)).files.length,0,'historical cache filename alone cannot authorize cleanup');
});

test('clean break confirms already absent captured keys without overwriting unrelated later edits',async()=>{
    const destination=path.join(spec.root,'.mcp.json'),source=JSON.stringify({mcpServers:{airk:unifiedEntry(spec,launchDescriptorPath(storage)),other:{command:'foreign'}}});
    const plan=await captureCleanBreakClientCleanup(spec.service!.installDir,storage,[{agent:'claude',destination,source}],[spec],()=>{});
    const afterManualRemoval=JSON.stringify({settings:'user-edited',mcpServers:{other:{command:'new-foreign-command'}}});let writes=0;
    const result=await applyCleanBreakClientCleanup(plan,spec.service!.installDir,new Set([destination]),()=>{},async()=>afterManualRemoval,async()=>{writes++;});
    assert.equal(result.complete,true);assert.equal(writes,0);assert.deepEqual(result.removed,[destination]);
});

test('clean break removes only native-owned HTTP credentials and preserves external declarations',async()=>{
    const original={command:path.resolve('old-helper'),args:['mcp']},raw=JSON.stringify({mcpServers:{saved:original}}),receipt=createHash('sha256').update(raw).digest('hex');
    const info=adoptedAliasInfo('saved','a'.repeat(32),receipt),source=migrateStandaloneHttpEntry('claude',raw,'saved',original,info,receipt);
    const destination=path.join(spec.root,'.mcp.json');let lookups=0;
    const lookup=predecessorHttpFixture(path.join(spec.service!.installDir,'old-control'),'ab'.repeat(32),info.connection_id);lookup.endpoint=info.endpoint;lookup.namespace=info.endpoint.namespace;lookup.snapshot.endpoint=info.endpoint;lookup.snapshot.namespace=info.endpoint.namespace;lookup.qualification.current_namespace=info.endpoint.namespace;
    const {snapshot,...proof}=lookup;
    const plan=await captureCleanBreakClientCleanup(spec.service!.installDir,storage,[{agent:'claude',destination,source}],[],()=>{},async(_agent,id)=>{lookups++;assert.equal(id,info.connection_id);return proof as any;});
    assert.equal(lookups,1);assert.deepEqual(plan.blockers,[]);assert.equal(plan.files[0].removals.length,1);
    let current=source;
    const result=await applyCleanBreakClientCleanup(plan,spec.service!.installDir,new Set([destination]),()=>{},async()=>current,async(_destination,_before,after)=>{current=after;});
    assert.equal(result.complete,true);assert.deepEqual(parse(current).mcpServers,{});assert.equal(lookups,1,'cleanup never enrolls or renews native credentials');
    const external=await captureCleanBreakClientCleanup(spec.service!.installDir,storage,[{agent:'claude',destination,source}],[],()=>{},async()=>undefined);
    assert.equal(external.files.length,0);assert.equal(external.blockers[0].code,'clean_break_client_ownership_unconfirmed');
});

test('startup replaces recognized legacy connections with one endpoint and preserves policy and unrelated servers',()=>{
    for(const agent of ['codex','claude','copilot'] as const){
        const assistance=assistanceEntry(spec,launchDescriptorPath(storage))!;
        const before=agent==='codex'?toml()+`
[mcp_servers.airk_explorer]
command = ${JSON.stringify(assistance.command)}
args = ${JSON.stringify(assistance.args)}
`:
            json(agent).replace('"other":',`"airk_explorer":${JSON.stringify({command:assistance.command,args:assistance.args})},"other":`);
        const after=migrateAgentConfig(agent,spec,storage,before);
        assert.equal(migrateAgentConfig(agent,spec,storage,after),after,'second startup makes no edits');
        assert.ok(!after.includes('airk_explorer')&&!after.includes(name),'both old server definitions are removed');
        const expected=unifiedEntry(spec,launchDescriptorPath(storage));
        if(agent==='codex'){
            assert.ok(after.startsWith('model = "user-model"'));
            assert.ok(after.includes('[mcp_servers.airk]')&&after.includes('[mcp_servers.airk.env]'));
            assert.ok(after.includes('enabled = false')&&after.includes('"AIRK_CLIENT" = "codex"'));
            assert.ok(after.includes('[mcp_servers.other]\ncommand = "other"'));
            assert.ok(after.includes(JSON.stringify(expected.args)));
        }else{
            const value=parse(after),servers=value[agent==='copilot'?'servers':'mcpServers'];
            assert.ok(after.startsWith('// Keep this comment'));
            assert.deepEqual(Object.keys(servers).sort(),['airk','other']);
            assert.deepEqual(servers.airk,{command:expected.command,args:expected.args,env:entry.env,disabled:true,...(agent==='copilot'?{type:'stdio'}:{})});
            assert.deepEqual(servers.other,{command:'other'});assert.deepEqual(value.inputs,[]);
        }
    }
});

test('disabling Explorer integration still consolidates servers, and custom conflicting entries preserve the entire file',()=>{
    for(const agent of ['codex','claude','copilot'] as const){
        const source=agent==='codex'?toml():json(agent);
        const after=migrateAgentConfig(agent,spec,storage,source,false);
        assert.ok(!after.includes('--explorer-descriptor')&&!after.includes('airk_explorer'));
        assert.ok(!after.includes('--service-project'));
    }
    const before=json('claude').replace('"other":', '"airk_explorer":{"command":"custom","args":[]},"other":');
    assert.throws(()=>migrateAgentConfig('claude',spec,storage,before),/Unrecognized/);
    assert.throws(()=>migrateAgentConfig('codex',spec,storage,toml()+'\n[mcp_servers.airk_explorer]\ncommand = "custom"\nargs = []\n'),/Unrecognized/);
});

test('tool policies are translated without preapproving additional combined actions',()=>{
    const old=assistanceEntry(spec,launchDescriptorPath(storage))!;
    const before=JSON.stringify({mcpServers:{[name]:{...entry,autoApprove:['airk_search'],enabled_tools:['airk_search','airk_status'],disabled_tools:['airk_read_file']},
        airk_explorer:{command:old.command,args:old.args,autoApprove:['airk_explorer_suggest_navigation'],enabled_tools:['airk_explorer_suggest_search'],env:{UI_SETTING:'keep'}}}});
    const warnings:string[]=[];
    const value=parse(migrateAgentConfig('claude',spec,storage,before,true,message=>warnings.push(message))).mcpServers.airk;
    assert.deepEqual(value.autoApprove,['search']);assert.equal(warnings.length,1);
    assert.deepEqual(value.enabled_tools,['search','status','draft_search']);
    assert.deepEqual(value.disabled_tools,['read_file']);
    assert.deepEqual(value.env,{AIRK_CLIENT:'codex',UI_SETTING:'keep'});
    const conflict=before.replace('"UI_SETTING":"keep"','"AIRK_CLIENT":"other"');
    assert.throws(()=>migrateAgentConfig('claude',spec,storage,conflict),/environments conflict/);
    assert.throws(()=>migrateAgentConfig('claude',spec,storage,before.replace('"airk_search"','"airk_*"')),/patterns require review/);
    assert.throws(()=>migrateAgentConfig('claude',spec,storage,json('claude',{...entry,disabled:'false'})),/enabled\/disabled/);
});

test('missing entries never enable an agent, and foreign/ambiguous launches are preserved',()=>{
    for(const agent of ['codex','claude','copilot'] as const){
        const empty=agent==='codex'?'model = "user-model"\n':'{}\n';
        assert.equal(migrateAgentConfig(agent,spec,storage,empty),empty);
        for(const foreign of [
            {...entry,command:path.resolve('foreign','airk.exe')},
            {...entry,args:entry.args.map(value=>value===spec.root?path.resolve('foreign-source'):value)},
            {...entry,args:[...entry.args,'--extra']},
        ])assert.throws(()=>migrateAgentConfig(agent,spec,storage,agent==='codex'?toml(foreign):json(agent,foreign)),/Unrecognized/);
    }
    assert.throws(()=>migrateAgentConfig('claude',spec,storage,json('claude',{...entry,url:'https://example.invalid'})),/Unrecognized/);
    assert.throws(()=>migrateAgentConfig('copilot',spec,storage,`{"servers":{"${name}":{},"${name}":{}}}`),/Duplicate/);
    for(const extra of ['command = "other"','"url" = "remote"','args = [\n"other"\n]']){
        assert.throws(()=>migrateAgentConfig('codex',spec,storage,toml().replace('enabled = false',extra)),/Unsupported|Duplicate|Unrecognized/);
    }
    assert.throws(()=>migrateAgentConfig('codex',spec,storage,toml()+`[mcp_servers.${name}]\n`),/Duplicate/);
});

test('versioned helper pointers migrate even when the old executable has been removed',()=>{
    const old={...entry,command:path.join(storage,'helpers','1.3.0-win32-x64-abc','airk.exe')};
    assert.ok(migrateAgentConfig('claude',spec,storage,json('claude',old)).includes('--root'));
});

test('Windows native namespaced roots match ordinary agent paths', {skip:process.platform!=='win32'},()=>{
    const native={...spec,root:path.toNamespacedPath(spec.root)};
    assert.ok(migrateAgentConfig('claude',native,storage,json('claude')).includes('--root'));
});

test('workspace migration skips missing files, refuses linked directories and reports failed saves',async context=>{
    // Native registration returns the physical source identity, including on Macs
    // whose temporary directory is reached through /var rather than /private/var.
    const root=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-agent-migration-')));
    context.after(()=>fs.rm(root,{recursive:true,force:true}));
    const local={...spec,root},old={...entry,args:entry.args.map(value=>value===spec.root?root:value)};
    await fs.mkdir(path.join(root,'.codex'));await fs.mkdir(path.join(root,'.vscode'));
    await fs.writeFile(path.join(root,'.codex','config.toml'),toml(old));
    await fs.writeFile(path.join(root,'.mcp.json'),json('claude',old));
    await fs.writeFile(path.join(root,'.vscode','mcp.json'),json('copilot',old));
    for(const parts of [['.gemini','settings.json'],['.agents','mcp_config.json'],['.cursor','mcp.json'],['.roo','mcp.json'],['.junie','mcp','mcp.json']]){
        const destination=path.join(root,...parts);await fs.mkdir(path.dirname(destination),{recursive:true});await fs.writeFile(destination,json('claude',old));
    }
    let saves=0;
    const messages=await migrateLocalAgents(local,storage,()=>{},async(file,before,after)=>{
        assert.equal(await fs.readFile(file,'utf8'),before);await fs.writeFile(file,after);saves++;
    });
    assert.equal(saves,8);assert.equal(messages.length,8);
    await migrateLocalAgents(local,storage,()=>{},async()=>{assert.fail('idempotent migration must not save');});
    await fs.writeFile(path.join(root,'.mcp.json'),json('claude',old));
    const failures=await migrateLocalAgents(local,storage,()=>{},async()=>{throw new Error('unsaved edits');});
    assert.ok(failures.some(message=>message.includes('unsaved edits')));
    const unconfirmed=await migrateLocalAgents(local,storage,()=>{},async()=>{});
    assert.ok(unconfirmed.some(message=>message.includes('save is unconfirmed')),'A fulfilled editor save callback is not disk persistence proof');
    assert.equal(await fs.readFile(path.join(root,'.mcp.json'),'utf8'),json('claude',old));
    await assert.rejects(migrateLocalAgents(local,storage,()=>{throw new Error('lost trust');},async()=>{}),/lost trust/);
    await fs.unlink(path.join(root,'.mcp.json'));
    await fs.rename(path.join(root,'.codex'),path.join(root,'redirected'));
    await fs.symlink(path.join(root,'redirected'),path.join(root,'.codex'),process.platform==='win32'?'junction':'dir');
    const linked=await migrateLocalAgents(local,storage,()=>{},async()=>{assert.fail('linked directory must not save');});
    assert.ok(linked.some(message=>message.includes('redirected')));
});

test('a recognized manual VS Code entry owns connection policy and suppresses duplicate provider registration',()=>{
    const entry=unifiedEntry(spec,launchDescriptorPath(storage));
    const source=JSON.stringify({servers:{airk:{command:entry.command,args:entry.args,disabled:true}}});
    assert.equal(hasUnifiedVscodeConfig(spec,storage,source),true);
    assert.equal(hasUnifiedVscodeConfig(spec,storage,json('copilot')),false);
    assert.equal(hasUnifiedVscodeConfig(spec,storage,source.replace(JSON.stringify(spec.root),JSON.stringify(path.resolve('foreign-root')))),false);
});

test('comments inside removed server entries survive consolidation and repeated startup',()=>{
    const before=json('claude').replace('"command":','/* Keep this server explanation */ "command":');
    const after=migrateAgentConfig('claude',spec,storage,before);
    assert.ok(after.includes('/* Keep this server explanation */'));
    assert.equal(migrateAgentConfig('claude',spec,storage,after),after);
    const old=toml().replace(`[mcp_servers.${name}]`,`[mcp_servers.${name}] # Keep table note`);
    const updated=migrateAgentConfig('codex',spec,storage,old);
    assert.ok(updated.includes('# Keep table note'));
    assert.equal(migrateAgentConfig('codex',spec,storage,updated),updated);
});

test('startup migrates recognized source scopes even when every prior local receipt is gone',()=>{
    const old={...spec,dataDir:path.resolve('data','service-1.4'),service:{...spec.service!,project:'cd'.repeat(32)}};
    const current={...spec,dataDir:path.resolve('data','index-v1')};
    for(const agent of ['codex','claude','copilot'] as const){
        const pinned=(value:LaunchSpec)=>({...unifiedEntry(value),args:['runtime','launch','--install-dir',value.service!.installDir,'--','--root',value.root,'--data-dir',value.dataDir,'--collection',value.collection,'--service-project',value.service!.project,'mcp']});
        const before=serializeServers(agent==='copilot'?'vscode':agent,[pinned(old)]);
        const automatic=migrateAgentConfig(agent,current,storage,before);assert.ok(!automatic.includes('--service-project')&&!automatic.includes('--data-dir'));
        const after=migrateAgentConfig(agent,current,storage,before,true,undefined,true);
        assert.ok(after.includes('--root')&&!after.includes('--data-dir')&&!after.includes('--service-project'));
        assert.ok(!after.includes('service-1.4')&&!after.includes(old.service.project));
        assert.equal(migrateAgentConfig(agent,current,storage,after,true,undefined,true),after);
        for(const foreign of [{...old,root:path.resolve('another-project')},{...old,collection:'another-collection'},
            {...old,command:path.resolve('foreign-launcher')}]){
            const source=serializeServers(agent==='copilot'?'vscode':agent,[pinned(foreign)]);
            assert.throws(()=>migrateAgentConfig(agent,current,storage,source,true,undefined,true),/Unrecognized/);
        }
    }
});

test('modern native HTTP refresh preserves complete scoped authority and policies without a synthetic source spec',()=>{
    const id='a'.repeat(32),directory=path.resolve('fictional-native-private'),url='https://127.0.0.1:57421/mcp';
    const info:any={schema:1,connection_id:id,endpoint:{schema:1,installation:'b'.repeat(64),user:'fixture',namespace:'c'.repeat(32),generation:3,service_instance:'d'.repeat(32),executable_sha256:'e'.repeat(64),url,ca_sha256:'f'.repeat(64),certificate_sha256:'1'.repeat(64),state:'ready',predecessor_digest:null},selection:{kind:'combined',selection:{schema:1,context_key:'2'.repeat(64),namespace:'c'.repeat(32),expected_catalog_revision:7,grant_id:'grt_'+'3'.repeat(32),expected_grant_revision:2,project_ids:['prj_'+'4'.repeat(32)],index_ids:['idx_'+'5'.repeat(32)]}},ca_certificate_file:path.join(directory,'ca.pem'),credential_file:path.join(directory,'credential-'+id+'.token')};
    const oldUrl='https://127.0.0.1:57422/mcp',variable='AIRK_MCP_TOKEN_'+id.toUpperCase();
    for(const agent of ['codex','claude','copilot'] as const){
        const source=agent==='codex'?`model = "keep-model"\n[mcp_servers.airk]\nurl = "${oldUrl}"\nbearer_token_env_var = "${variable}"\nenabled = false\n`:JSON.stringify({keep:'user', [agent==='copilot'?'servers':'mcpServers']:{airk:{type:'http',url:oldUrl,headers:{Authorization:agent==='copilot'?'Bearer ${input:airkToken_'+id+'}':'Bearer ${'+variable+'}','X-AIRK-Response-Mode':'full'},disabled:true},foreign:{command:'other'}}});
        const before=structuredClone(info),after=refreshHttpAgentConfig(agent,source,info,oldUrl);assert.ok(after.includes(url));assert.ok(!after.includes(oldUrl));assert.deepEqual(info,before);
        if(agent==='codex'){assert.ok(after.includes('keep-model'));assert.ok(after.includes('enabled = false'));assert.equal(httpConfigurationConnectionId(agent,{url:oldUrl,bearer_token_env_var:variable}),id);}
        else{const value=parse(after),entry=value[agent==='copilot'?'servers':'mcpServers'].airk;assert.equal(entry.disabled,true);assert.equal(value.keep,'user');assert.deepEqual(value[agent==='copilot'?'servers':'mcpServers'].foreign,{command:'other'});assert.equal(httpConfigurationConnectionId(agent,entry),id);}
        assert.throws(()=>refreshHttpAgentConfig(agent,source,{...info,connection_id:'0'.repeat(32),credential_file:path.join(directory,'credential-'+'0'.repeat(32)+'.token')},oldUrl));
    }
    assert.equal(httpConfigurationConnectionId('claude',{url:'https://example.invalid/mcp',headers:{Authorization:'Bearer ${'+variable+'}'}}),undefined);
});

function adoptedAliasInfo(serverKey:string,id:string,configurationReceipt:string,container=['mcpServers']):any{
    const directory=path.resolve('fictional-native-private'),namespace='c'.repeat(32),root=path.resolve('saved-original-'+id[0]),original={binding:'1'.repeat(64),root,data_root:path.resolve('original-data-'+id[0]),collection:'original-'+id[0]},registration={...original,binding:'2'.repeat(64),data_root:path.resolve('adopted-data-'+id[0])};
    const setup={caller:'d'.repeat(32),key:{namespace,epoch:1,nonce:'e'.repeat(32)},expected_catalog_revision:1,name:'Saved original',registration,sources:[{canonical_path:root,owner:true,aliases:[]}],setup_watch:false};
    const digest='f'.repeat(64),plan={schema:1,setup,original_registration:original,origin_sha256:digest,config_sha256:configurationReceipt,executable_sha256:digest,quiescence_sha256:digest,archive_inventory_sha256:digest,original_publication_sha256:digest,source_inventory_sha256:digest};
    const adoption={schema:1,operation_id:'d'.repeat(32),plan,publication:{schema:1,plan_digest:digest,archive_inventory_sha256:digest,original_publication_sha256:digest,current:`airk-r1:${namespace}:2:${digest}`,resource_sha256:digest,compatibility_sha256:digest},original_server:serverKey,no_default_project:true,configuration_claims:[{schema:1,origin_sha256:plan.origin_sha256,config_sha256:configurationReceipt,executable_sha256:plan.executable_sha256,selected_entry_sha256:digest,container_path:container,original_server:serverKey,no_default_project:true}]};
    return {schema:1,connection_id:id,endpoint:{schema:1,installation:digest,user:'fixture',namespace,generation:1,service_instance:'d'.repeat(32),executable_sha256:digest,url:'https://127.0.0.1:57421/mcp',ca_sha256:digest,certificate_sha256:digest,state:'ready',predecessor_digest:null},selection:{kind:'imported_legacy',selection:{namespace,expected_catalog_revision:2,registration,configuration_receipt:configurationReceipt,grant:{grant:{id:'grt_'+'91'.repeat(16),host:digest,revision:1,revoked:false,memberships:[]},provenance:{kind:'imported_standalone_adoption',adoption}}}},ca_certificate_file:path.join(directory,'ca.pem'),credential_file:path.join(directory,'credential-'+id+'.token')};
}
for(const agent of ['codex','claude','copilot'] as const)test(`native standalone ${agent} aliases retain disjoint scopes, original names, comments and policies`,()=>{
    const keys=['old.first','second_old'],fields=keys.map((key,index)=>({command:path.resolve('original-helper-'+index),args:['--root',path.resolve('source-'+index),'--collection','saved-'+index,'mcp'],env:{AIRK_CLIENT:agent,AIRK_MCP_RESPONSE_MODE:index?'terse':'full'},disabled:true,enabled_tools:['airk_search'],custom_setting:'keep-'+key}));
    const prefix=agent==='codex'?'instructions = """unrelated instructions\n[mcp_servers.fake]\nstay unchanged"""\n':'// Original comment remains\n';
    const before=agent==='codex'?prefix+fields.map((entry,index)=>`[mcp_servers.${JSON.stringify(keys[index])}]\ncommand = ${JSON.stringify(entry.command)}\nargs = ${JSON.stringify(entry.args)}\ndisabled = true\nenabled_tools = ["airk_search"]\ncustom_setting = ${JSON.stringify(entry.custom_setting)}\n[mcp_servers.${JSON.stringify(keys[index])}.env]\nAIRK_CLIENT = ${JSON.stringify(agent)}\nAIRK_MCP_RESPONSE_MODE = ${JSON.stringify(entry.env.AIRK_MCP_RESPONSE_MODE)}\n`).join('')+'[mcp_servers.airk]\ncommand = "foreign"\n':prefix+JSON.stringify({keep:'unchanged',[agent==='copilot'?'servers':'mcpServers']:{[keys[0]]:fields[0],[keys[1]]:fields[1],airk:{command:'foreign'}}});
    const digest=createHash('sha256').update(before).digest('hex'),originalRows=globalConfigDeclarations(agent,before),connections=keys.map((key,index)=>adoptedAliasInfo(key,(index?'b':'a').repeat(32),digest,[agent==='codex'?'mcp_servers':agent==='copilot'?'servers':'mcpServers']));
    let after=before;
    for(const [index,key]of keys.entries())after=migrateStandaloneHttpEntry(agent,after,key,originalRows.find(row=>row.serverKey===key)!.fields,connections[index],digest);
    assert.ok(after.startsWith(prefix));const rows=globalConfigDeclarations(agent,after);
    assert.deepEqual(rows.find(row=>row.serverKey==='airk')?.fields,originalRows.find(row=>row.serverKey==='airk')?.fields);
    for(const [index,key]of keys.entries()){
        const row=rows.find(row=>row.serverKey===key)!;assert.ok(row);assert.equal(row.fields.command,undefined);assert.equal(row.fields.url,connections[index].endpoint.url);assert.equal(row.fields.disabled,true);assert.deepEqual(row.fields.enabled_tools,['search']);assert.equal(row.fields.custom_setting,fields[index].custom_setting);
        const headers=row.fields[agent==='codex'?'http_headers':'headers'] as Record<string,string>;assert.equal(headers['X-AIRK-Response-Mode'],index?'terse':'full');assert.equal(httpConfigurationConnectionId(agent,row.fields),connections[index].connection_id);
    }
    const next={...connections[0],endpoint:{...connections[0].endpoint,url:'https://127.0.0.1:57422/mcp'}};
    const refreshed=refreshHttpAgentConfig(agent,after,next,connections[0].endpoint.url,keys[0]),nextRows=globalConfigDeclarations(agent,refreshed);
    assert.equal(nextRows.find(row=>row.serverKey===keys[0])!.fields.url,next.endpoint.url);assert.deepEqual(nextRows.find(row=>row.serverKey===keys[1])!.fields,rows.find(row=>row.serverKey===keys[1])!.fields);
    assert.throws(()=>refreshHttpAgentConfig(agent,after,connections[1],connections[0].endpoint.url,keys[0]),/alias_unconfirmed/);
    assert.throws(()=>migrateStandaloneHttpEntry(agent,before,keys[0],{...fields[0],command:'crossed'},connections[0],digest),/Unrecognized/);
    assert.throws(()=>migrateStandaloneHttpEntry(agent,before,keys[0],fields[0],connections[1],digest),/origin_unconfirmed/);
    assert.throws(()=>migrateStandaloneHttpEntry(agent,before,keys[0],fields[0],connections[0],'0'.repeat(64)),/origin_unconfirmed/);
});

test('a pre-service native alias literally named airk translates wildcard approval only from the old retrieval catalog',()=>{
    const original={command:path.resolve('old-helper'),args:['mcp'],alwaysAllow:['*']},source=JSON.stringify({mcpServers:{airk:original}}),digest=createHash('sha256').update(source).digest('hex'),info=adoptedAliasInfo('airk','a'.repeat(32),digest);
    const after=migrateStandaloneHttpEntry('claude',source,'airk',original,info,digest),value=parse(after);
    assert.deepEqual(value.mcpServers.airk.alwaysAllow,[...RETRIEVAL_TOOLS]);assert.ok(!value.mcpServers.airk.alwaysAllow.includes('draft_plan'));
});

for(const agent of ['codex','claude','copilot'] as const)test(`standalone ${agent} reattachment replaces only the exact owned credential under unchanged native authority`,()=>{
    const alias='old_saved',fields={command:path.resolve('old-helper'),args:['mcp'],disabled:true,enabled_tools:['airk_search'],custom_setting:'keep-original-policy'};
    const source=agent==='codex'?`# Keep original comment\n[mcp_servers.old_saved]\ncommand = ${JSON.stringify(fields.command)}\nargs = ["mcp"]\ndisabled = true\nenabled_tools = ["airk_search"]\ncustom_setting = "keep-original-policy"\n`:JSON.stringify({keep:'foreign',[agent==='copilot'?'servers':'mcpServers']:{[alias]:fields,other:{command:'other-tool'}}});
    const receipt=createHash('sha256').update(source).digest('hex'),old=adoptedAliasInfo(alias,'a'.repeat(32),receipt,[agent==='codex'?'mcp_servers':agent==='copilot'?'servers':'mcpServers']);
    const nested=structuredClone(old.selection);nested.selection.grant.provenance.adoption.plan.setup.expected_catalog_revision++;
    assert.notEqual(httpAuthorityDigest(nested),httpAuthorityDigest(old.selection),'Only the selected minimum clock is normalized; original adoption intent stays exact');
    const before=migrateStandaloneHttpEntry(agent,source,alias,fields,old,receipt),next=structuredClone(old);next.connection_id='b'.repeat(32);next.credential_file=path.join(path.dirname(old.credential_file),'credential-'+next.connection_id+'.token');next.endpoint.url='https://127.0.0.1:57422/mcp';next.endpoint.generation++;
    const untouched=structuredClone({old,next}),after=refreshHttpAgentConfig(agent,before,next,old.endpoint.url,alias,old),rows=globalConfigDeclarations(agent,after),row=rows.find(value=>value.serverKey===alias)!;
    assert.equal(httpConfigurationConnectionId(agent,row.fields),next.connection_id);assert.equal(row.fields.url,next.endpoint.url);assert.equal(row.fields.disabled,true);assert.deepEqual(row.fields.enabled_tools,['search']);assert.equal(row.fields.custom_setting,fields.custom_setting);assert.deepEqual({old,next},untouched);
    if(agent==='codex')assert.ok(after.startsWith('# Keep original comment'));else assert.deepEqual(JSON.parse(JSON.stringify(rows.find(value=>value.serverKey==='other')?.fields)),{command:'other-tool'});
    for(const mutate of [
        (value:any)=>{value.endpoint.installation='0'.repeat(64);},
        (value:any)=>{value.endpoint.user='other-user';},
        (value:any)=>{value.selection.selection.grant.grant.id='grt_'+'0'.repeat(32);},
        (value:any)=>{value.selection.selection.registration.data_root=path.resolve('crossed-backing');},
        (value:any)=>{value.selection.selection.grant.provenance.adoption.configuration_claims[0].origin_sha256='0'.repeat(64);},
    ]){const crossed=structuredClone(next);mutate(crossed);assert.throws(()=>refreshHttpAgentConfig(agent,before,crossed,old.endpoint.url,alias,old));}
    assert.throws(()=>refreshHttpAgentConfig(agent,before,next,'https://127.0.0.1:57423/mcp',alias,old),/reattachment_unconfirmed/);
    assert.throws(()=>refreshHttpAgentConfig(agent,before.replaceAll(old.connection_id, 'c'.repeat(32)).replaceAll(old.connection_id.toUpperCase(),'C'.repeat(32)),next,old.endpoint.url,alias,old),/Unrecognized/);
});
