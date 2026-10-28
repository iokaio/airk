// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { parse } from 'jsonc-parser';
import { LaunchSpec, unifiedEntry, assistanceEntry, serializeServers } from '../model';
import { launchDescriptorPath } from '../explorerAssistanceLaunch';
import { LocalAgent, hasUnifiedVscodeConfig, migrateAgentConfig, migrateLocalAgents } from '../agentMigration';

const storage=path.resolve('fixture-storage');
const spec:LaunchSpec={root:path.resolve('fixture-source'),dataDir:path.resolve('fresh-data'),collection:'project-fixture',name:'fixture',
    command:path.resolve('installed-runtime','airk-launcher'),service:{installDir:path.resolve('installed-runtime'),project:'ab'.repeat(32)}};
const name='airk-project-fixture';
const entry={command:path.join(storage,'cli','bin',process.platform==='win32'?'airk.exe':'airk'),
    args:['--root',spec.root,'--data-dir',path.resolve('old-data'),'--collection',spec.collection,'mcp'],env:{AIRK_CLIENT:'codex'},disabled:true};
function json(agent:LocalAgent,value:unknown=entry):string{return `// Keep this comment\n${JSON.stringify({[agent==='copilot'?'servers':'mcpServers']:{[name]:value,other:{command:'other'}},inputs:[]},null,2)}\n`;}
function toml(value=entry):string{return `model = "user-model"\n[mcp_servers.${name}]\ncommand = ${JSON.stringify(value.command)}\nargs = ${JSON.stringify(value.args)}\nenabled = false\n[mcp_servers.${name}.env]\nAIRK_CLIENT = "codex"\n[mcp_servers.other]\ncommand = "other"\n`;}

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
        assert.ok(after.includes(spec.service!.project));
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
    assert.ok(migrateAgentConfig('claude',spec,storage,json('claude',old)).includes(spec.service!.project));
});

test('Windows native namespaced roots match ordinary agent paths', {skip:process.platform!=='win32'},()=>{
    const native={...spec,root:path.toNamespacedPath(spec.root)};
    assert.ok(migrateAgentConfig('claude',native,storage,json('claude')).includes(spec.service!.project));
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
    assert.equal(hasUnifiedVscodeConfig(spec,storage,source.replace(spec.service!.project,'cd'.repeat(32))),false);
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

test('explicit connection update replaces old storage and binding together, while startup preserves identity',()=>{
    const old={...spec,dataDir:path.resolve('data','service-1.4'),service:{...spec.service!,project:'cd'.repeat(32)}};
    const current={...spec,dataDir:path.resolve('data','index-v1')};
    for(const agent of ['codex','claude','copilot'] as const){
        const before=serializeServers(agent==='copilot'?'vscode':agent,[unifiedEntry(old,launchDescriptorPath(storage))]);
        assert.throws(()=>migrateAgentConfig(agent,current,storage,before),/Unrecognized/,'background migration must not silently rebind');
        const after=migrateAgentConfig(agent,current,storage,before,true,undefined,true);
        assert.ok(after.includes('index-v1')&&after.includes(current.service!.project));
        assert.ok(!after.includes('service-1.4')&&!after.includes(old.service.project));
        assert.equal(migrateAgentConfig(agent,current,storage,after,true,undefined,true),after);
        for(const foreign of [{...old,root:path.resolve('another-project')},{...old,collection:'another-collection'},
            {...old,command:path.resolve('foreign-launcher')},{...old,dataDir:current.dataDir}]){
            const source=serializeServers(agent==='copilot'?'vscode':agent,[unifiedEntry(foreign,launchDescriptorPath(storage))]);
            assert.throws(()=>migrateAgentConfig(agent,current,storage,source,true,undefined,true),/Unrecognized/);
        }
    }
});
