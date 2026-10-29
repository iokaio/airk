// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import { rememberUninstallTarget, uninstallTargets } from '../extensionUninstall';
import { LaunchSpec, unifiedEntry } from '../model';
import { launchDescriptorPath } from '../explorerAssistanceLaunch';
import { NativeRunner } from '../serviceBootstrap';

async function fixture(){
    const temporary=await fs.mkdtemp(path.join(os.tmpdir(),'airk-uninstall-hook-')),root=await fs.realpath(temporary);
    const installDir=path.join(root,'custom-runtime'),launcher=path.join(root,'launcher'),source=path.join(root,'project'),storage=path.join(root,'editor');
    for(const dir of [installDir,launcher,source,storage])await fs.mkdir(dir);
    const spec:LaunchSpec={command:path.join(launcher,process.platform==='win32'?'airk-launcher.exe':'airk-launcher'),root:source,dataDir:path.join(root,'data'),collection:'fixture',name:'Fixture',
        service:{installDir,project:'12'.repeat(32),universal:true}};
    const calls:string[][]=[],messages:string[]=[];
    let fail=false;
    const run:NativeRunner=async(_main,args)=>{
        calls.push(args);
        if(args[1]==='location')return {protocol:1,install_dir:path.join(root,args[3]+'-default'),user_paths:{launcher:args[3]==='stable'?launcher:path.join(root,'preview-launcher')}};
        assert.deepEqual(args,['runtime','uninstall','--install-dir',installDir]);
        return {uninstalled:!fail,project_data_preserved:true};
    };
    return {root,installDir,launcher,source,storage,spec,calls,messages,run,setFailure:(value:boolean)=>{fail=value;},
        cleanup:()=>fs.rm(root,{recursive:true,force:true})};
}
test('uninstall removes owned Claude/Codex/Copilot connections in a closed project and delegates runtime shutdown',async()=>{
    const f=await fixture();
    try{
        await rememberUninstallTarget(f.launcher,f.installDir,f.storage);
        await rememberUninstallTarget(f.launcher,f.installDir,f.storage,f.spec);
        const entry=unifiedEntry(f.spec,launchDescriptorPath(f.storage));
        await fs.mkdir(path.join(f.source,'.codex'));await fs.mkdir(path.join(f.source,'.vscode'));
        const toml='# Preserve project policy\nmodel = "chosen-model"\n[mcp_servers.airk]\ncommand = '+JSON.stringify(entry.command)+'\nargs = '+JSON.stringify(entry.args)+'\n[mcp_servers.airk.env]\nMY_OPTION = "yes"\n[mcp_servers.other]\ncommand = "keep-me"\n';
        await fs.writeFile(path.join(f.source,'.codex','config.toml'),toml);
        await fs.writeFile(path.join(f.source,'.mcp.json'),JSON.stringify({mcpServers:{airk:entry,other:{command:'keep-me'}}}));
        await fs.writeFile(path.join(f.source,'.vscode','mcp.json'),JSON.stringify({servers:{airk:entry,other:{command:'keep-me'}}}));
        await uninstallTargets('verified-bundle',f.run,message=>f.messages.push(message));
        const codex=await fs.readFile(path.join(f.source,'.codex','config.toml'),'utf8');
        assert.doesNotMatch(codex,/mcp_servers\.airk/);assert.match(codex,/model = "chosen-model"/);assert.match(codex,/mcp_servers\.other/);
        assert.match(codex,/Preserve project policy/);
        for(const [file,group] of [['.mcp.json','mcpServers'],['.vscode/mcp.json','servers']]){
            const value=JSON.parse(await fs.readFile(path.join(f.source,file),'utf8'));
            assert.deepEqual(value[group],{other:{command:'keep-me'}});
        }
        assert.equal(f.calls.filter(args=>args[1]==='uninstall').length,1);
        assert.deepEqual(await fs.readdir(path.join(f.launcher,'extension-uninstall')),[]);
        assert.ok(f.messages.some(message=>message.startsWith('AIRK runtime and recorded')));
        // Re-running the hook cannot add an AIRK connection or start a service.
        await uninstallTargets('verified-bundle',f.run,()=>{});
        assert.equal(f.calls.some(args=>['ensure','start','stage','activate'].includes(args[1])),false);
    }finally{await f.cleanup();}
});
test('uninstall preserves foreign connections and retains receipts when native cleanup is incomplete',async()=>{
    const f=await fixture();
    try{
        await rememberUninstallTarget(f.launcher,f.installDir,f.storage,f.spec);
        const file=path.join(f.source,'.mcp.json'),before=JSON.stringify({mcpServers:{airk:{command:'another-server',args:[]}}});
        await fs.writeFile(file,before);f.setFailure(true);
        await assert.rejects(uninstallTargets('verified-bundle',f.run,message=>f.messages.push(message)),/Runtime uninstall incomplete/);
        assert.equal(await fs.readFile(file,'utf8'),before);
        assert.equal((await fs.readdir(path.join(f.launcher,'extension-uninstall'))).length,1);
        assert.ok(f.messages.some(message=>message.startsWith('Could not remove')));
        assert.equal(f.messages.some(message=>message.startsWith('AIRK runtime and recorded')),false);
    }finally{await f.cleanup();}
});
test('a tampered uninstall receipt cannot nominate a different installation',async()=>{
    const f=await fixture();
    try{
        await rememberUninstallTarget(f.launcher,f.installDir,f.storage);
        const directory=path.join(f.launcher,'extension-uninstall'),file=path.join(directory,(await fs.readdir(directory))[0]);
        const receipt=JSON.parse(await fs.readFile(file,'utf8'));receipt.installDir=path.join(f.root,'foreign-runtime');await fs.writeFile(file,JSON.stringify(receipt));
        await assert.rejects(uninstallTargets('verified-bundle',f.run,()=>{}),/receipt identity changed/);
        assert.equal(f.calls.some(args=>args[1]==='uninstall'),false);
    }finally{await f.cleanup();}
});
