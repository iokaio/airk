// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { serviceTestHost } from './serviceTestHost';
import { LaunchSpec, serializeServers, unifiedEntry, UNIFIED_TOOLS } from '../model';
import * as native from '../native';
import { migrateAgentConfig } from '../agentMigration';

test('startup adopts the current native registration and preserves a manually corrected agent file',async context=>{
    const f=await serviceTestHost(context);
    try{
        const old=[...f.registrations.values()][0],current={...old,binding:'56'.repeat(32),data_root:path.join(f.root,'index-v1')};
        f.registrations.clear();f.registrations.set(current.binding,current);
        const spec:LaunchSpec={root:current.root,dataDir:current.data_root,collection:current.collection,name:'fixture',command:f.binding.launcher,
            service:{installDir:f.binding.installDir,project:current.binding}};
        const destination=path.join(current.root,'.codex','config.toml');await fs.mkdir(path.dirname(destination));
        const correct=migrateAgentConfig('codex',spec,f.context.globalStorageUri.fsPath,serializeServers('codex',[unifiedEntry(spec)]),false);
        await fs.writeFile(destination,correct);
        f.values.set(`airk.installComplete.1.${f.binding.installDir}`,`1.4.0:${f.binding.runtimeSha256}`);
        f.settings.set('cli.autoInstall',true);const calls=f.calls.length;
        await f.refreshFolders();
        assert.equal(await fs.readFile(destination,'utf8'),correct);
        assert.ok(!f.calls.slice(calls).some(line=>line.startsWith('Updated ')||line.startsWith('Could not update')),f.calls.slice(calls).join('\n'));
        const [definition]=await f.api.getServerDefinitions();
        assert.ok(definition.args.includes(current.root));assert.ok(!definition.args.includes('--service-project'));assert.ok(!definition.args.includes('--data-dir'));
        assert.ok(!definition.args.includes(old.binding));assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('Codex update saves and verifies a new native binding with Windows editor newlines, and reports failures',async context=>{
    const f=await serviceTestHost(context);
    try{
        const old=[...f.registrations.values()][0];
        const current={...old,binding:'ef'.repeat(32),data_root:path.join(f.root,'index-v1')};
        f.registrations.clear();f.registrations.set(current.binding,current);
        const oldSpec:LaunchSpec={root:old.root,dataDir:old.data_root,collection:old.collection,name:'fixture',command:f.binding.launcher,
            service:{installDir:f.binding.installDir,project:old.binding}};
        const destination=path.join(old.root,'.codex','config.toml');await fs.mkdir(path.dirname(destination));
        const before='# Keep my preferences\r\nmodel = "fixture"\r\n'+serializeServers('codex',[{...unifiedEntry(oldSpec),args:['runtime','launch','--install-dir',oldSpec.service!.installDir,'--','--root',oldSpec.root,'--data-dir',oldSpec.dataDir,'--collection',oldSpec.collection,'--service-project',oldSpec.service!.project,'mcp']}]).replace(/\r?\n/g,'\r\n');
        await fs.writeFile(destination,before);
        let text=before,saves=0,checks=0,failProbe=false;
        const document={uri:{fsPath:destination},eol:2,isDirty:false,version:1,getText:()=>text,positionAt:(offset:number)=>offset,
            save:async()=>{await fs.writeFile(destination,text);document.isDirty=false;saves++;return true;}};
        f.stub.workspace.textDocuments=[document];
        f.stub.workspace.openTextDocument=async()=>document;
        f.stub.workspace.applyEdit=async(edit:{after:string})=>{text=edit.after.replace(/\r?\n/g,'\r\n');document.version++;document.isDirty=true;return true;};
        context.mock.method(native,'probeMcp',async(spec:LaunchSpec)=>{
            checks++;assert.equal(spec.dataDir,current.data_root);assert.equal(spec.service!.project,current.binding);
            assert.ok(!(await fs.readFile(destination,'utf8')).includes('--service-project'),'probe follows actual root-based disk save');
            if(failProbe)throw new Error('fixture connection timeout');
            return {tools:[...UNIFIED_TOOLS],status:{}};
        });
        f.state.pick=items=>items.find(item=>item.client==='codex');
        await f.commands.get('airk.setupAgent')!();
        const panel=f.panels.find(panel=>panel.viewType==='airk.connection')!;
        assert.match(panel.webview.html,/index-v1/);
        assert.match(panel.webview.html,/data-action="updateCodex"/,'updates work without Explorer assistance');
        await panel.send({action:'updateCodex'});
        assert.equal(saves,1,f.calls.join('\n'));assert.equal(checks,1);
        assert.match(text,/Keep my preferences/);assert.match(text,/model = "fixture"/);
        assert.ok(!text.includes(old.binding));assert.ok(text.includes('--root')&&!text.includes('--data-dir'));
        assert.match(panel.webview.html,/configuration saved.*27 tools/);
        await panel.send({action:'updateCodex'});
        assert.equal(saves,1);assert.equal(checks,2);assert.match(panel.webview.html,/already matches/);
        failProbe=true;await panel.send({action:'updateCodex'});
        assert.equal(saves,1);assert.match(panel.webview.html,/connection check failed.*fixture connection timeout/);
        assert.ok(f.calls.some(message=>message.includes('connection check failed')));
        failProbe=false;document.isDirty=true;
        await panel.send({action:'updateCodex'});
        assert.equal(saves,1);assert.equal(checks,3);assert.match(panel.webview.html,/unsaved edits/);
        assert.ok(f.calls.some(message=>message.includes('connection action failed')));
        document.isDirty=false;
        await panel.send({action:'createCodex'});
        assert.equal(saves,1);assert.equal(checks,3);assert.match(panel.webview.html,/already exists and was preserved/);
        assert.equal(await fs.readFile(destination,'utf8'),text);
        f.registrations.clear();f.registrations.set('12'.repeat(32),{...current,binding:'12'.repeat(32),data_root:path.join(f.root,'replacement-data')});
        await panel.send({action:'updateCodex'});
        assert.equal(saves,1);assert.equal(checks,3,'a changed registration cannot validate or save the old guide');
        assert.match(panel.webview.html,/Workspace or storage changed/);
        assert.equal(await fs.readFile(destination,'utf8'),text);
    }finally{await f.close();}
});
