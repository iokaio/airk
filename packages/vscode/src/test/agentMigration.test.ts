// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { parse } from 'jsonc-parser';
import { LaunchSpec, launchArgs, assistanceEntry } from '../model';
import { launchDescriptorPath } from '../explorerAssistanceLaunch';
import { LocalAgent, migrateAgentConfig, migrateLocalAgents } from '../agentMigration';

const storage=path.resolve('fixture-storage');
const spec:LaunchSpec={root:path.resolve('fixture-source'),dataDir:path.resolve('fresh-data'),collection:'project-fixture',name:'fixture',
    command:path.resolve('installed-runtime','airk-launcher'),service:{installDir:path.resolve('installed-runtime'),project:'ab'.repeat(32)}};
const name='airk-project-fixture';
const entry={command:path.join(storage,'cli','bin',process.platform==='win32'?'airk.exe':'airk'),
    args:['--root',spec.root,'--data-dir',path.resolve('old-data'),'--collection',spec.collection,'mcp'],env:{AIRK_CLIENT:'codex'},disabled:true};
function json(agent:LocalAgent,value:unknown=entry):string{return `// Keep this comment\n${JSON.stringify({[agent==='copilot'?'servers':'mcpServers']:{[name]:value,other:{command:'other'}},inputs:[]},null,2)}\n`;}
function toml(value=entry):string{return `model = "user-model"\n[mcp_servers.${name}]\ncommand = ${JSON.stringify(value.command)}\nargs = ${JSON.stringify(value.args)}\nenabled = false\n[mcp_servers.${name}.env]\nAIRK_CLIENT = "codex"\n[mcp_servers.other]\ncommand = "other"\n`;}

test('Codex, Claude Code and Copilot preserve policies, add missing Assistance and remain idempotent',()=>{
    for(const agent of ['codex','claude','copilot'] as const){
        const before=agent==='codex'?toml():json(agent);
        const after=migrateAgentConfig(agent,spec,storage,before);
        assert.ok(after.includes('airk-launcher'));assert.ok(after.includes(spec.service!.project));
        assert.equal(migrateAgentConfig(agent,spec,storage,after),after);
        if(agent==='codex'){
            assert.ok(after.startsWith('model = "user-model"'));assert.ok(after.includes('enabled = false'));
            const tail=after.slice(after.indexOf(`[mcp_servers.${name}.env]`),after.indexOf('[mcp_servers.airk_explorer]')).trimEnd();
            assert.equal(tail,before.slice(before.indexOf(`[mcp_servers.${name}.env]`)).trimEnd());
            assert.ok(after.slice(after.indexOf('[mcp_servers.airk_explorer]')).includes('enabled = false'));
        }else{
            assert.ok(after.startsWith('// Keep this comment'));
            const value=parse(after),servers=value[agent==='copilot'?'servers':'mcpServers'];
            assert.deepEqual(servers[name],{...entry,command:spec.command,args:launchArgs(spec)});
            assert.deepEqual(servers.other,{command:'other'});assert.deepEqual(value.inputs,[]);
            const assistance=assistanceEntry(spec,launchDescriptorPath(storage))!;
            assert.deepEqual(servers.airk_explorer,{command:assistance.command,args:assistance.args,disabled:true});
        }
    }
});

test('Assistance backfill honors the disable setting and never replaces a foreign companion server',()=>{
    for(const agent of ['codex','claude','copilot'] as const){
        const source=agent==='codex'?toml():json(agent);
        assert.ok(!migrateAgentConfig(agent,spec,storage,source,false).includes('airk_explorer'));
    }
    const before=json('claude').replace('"other":', '"airk_explorer":{"command":"custom","args":[]},"other":');
    const warnings:string[]=[];
    const after=migrateAgentConfig('claude',spec,storage,before,true,value=>warnings.push(value));
    assert.deepEqual(parse(after).mcpServers.airk_explorer,{command:'custom',args:[]});assert.equal(warnings.length,1);
    assert.throws(()=>migrateAgentConfig('codex',spec,storage,toml()+'\n[mcp_servers.airk_explorer]\ncommand = "custom"\nargs = []\n'),/manual review/);
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
        assert.throws(()=>migrateAgentConfig('codex',spec,storage,toml().replace('enabled = false',extra)),/Unsupported/);
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
    let saves=0;
    const messages=await migrateLocalAgents(local,storage,()=>{},async(file,before,after)=>{
        assert.equal(await fs.readFile(file,'utf8'),before);await fs.writeFile(file,after);saves++;
    });
    assert.equal(saves,3);assert.equal(messages.length,3);
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
