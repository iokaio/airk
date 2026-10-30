// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { serviceTestHost } from './serviceTestHost';
import * as cliInstall from '../cliInstall';
import { assistanceEntry, retrievalEntry, serializeServers, type LaunchSpec } from '../model';
import { launchDescriptorPath } from '../explorerAssistanceLaunch';
import { runText } from '../native';
import { NativeExecutionDisabled, setNativeExecutionGate } from '../nativeGate';
import { NativeControlTransport } from '../serviceClient';
import { McpStdioSession } from '../mcpStdioSession';

test('verified reopen migrates both old definitions quietly before publishing the connection',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        const project=[...f.registrations.values()][0];
        const spec:LaunchSpec={command:f.binding.launcher,root:project.root,dataDir:project.data_root,collection:project.collection,name:'fixture',service:{installDir:f.binding.installDir,project:project.binding}};
        const storage=path.join(f.root,'editor-storage');
        const before=serializeServers('claude',[retrievalEntry(spec),assistanceEntry(spec,launchDescriptorPath(storage))!]);
        const destination=path.join(project.root,'.mcp.json');await fs.writeFile(destination,before);
        let saves=0;
        const document={uri:{fsPath:destination},isDirty:false,version:1,text:before,getText(){return this.text;},positionAt:(offset:number)=>offset,
            async save(){saves++;await fs.writeFile(destination,this.text);return true;}};
        f.stub.workspace.openTextDocument=async()=>document;
        f.stub.workspace.applyEdit=async(edit:any)=>{document.text=edit.after;document.version++;return true;};
        const progress=f.progressRuns.length,writes=f.writes.length;
        await f.refreshFolders();
        const after=JSON.parse(await fs.readFile(destination,'utf8'));
        assert.deepEqual(Object.keys(after.mcpServers),['airk']);assert.equal(saves,1);
        assert.equal(f.progressRuns.length,progress,'completed setup remains quiet');
        assert.equal(f.writes.length,writes,'connection migration never queues another index');
        await f.refreshFolders();assert.equal(saves,1,'the next reopen does not rewrite the file');
    }finally{await f.close();}
});

test('missing Explorer assets fail explicitly and a later open retries after repair',async context=>{
    const f=await serviceTestHost(context,{trusted:false});
    try{
        const installed=f.context.extensionUri;
        f.context.extensionUri={scheme:'file',fsPath:path.join(f.root,'missing-explorer')};
        await assert.rejects(f.commands.get('airk.explorer.open')!(),/ENOENT/);
        assert.equal(f.panels.length,0);
        f.context.extensionUri=installed;
        await f.commands.get('airk.explorer.open')!();
        assert.equal(f.panels.length,1,'repair permits another load, including without workspace trust');
        assert.equal(f.writes.length,0,'opening Explorer does not start native indexing');
    }finally{await f.close();}
});

test('Install or Repair CLI migrates all three existing local agent connections through guarded editor saves',async context=>{
    const f=await serviceTestHost(context,{assistance:true});
    try{
        context.mock.method(cliInstall,'installLauncherCli',async()=>['Old CLI is locked; disconnect clients and retry cleanup.']);
        const project=[...f.registrations.values()][0],name=`airk-${project.collection}`;
        const command=path.join(f.root,'editor-storage','cli','bin',process.platform==='win32'?'airk.exe':'airk');
        const args=['--root',project.root,'--data-dir',path.join(f.root,'old-data'),'--collection',project.collection,'mcp'];
        const files=[path.join(project.root,'.codex','config.toml'),path.join(project.root,'.mcp.json'),path.join(project.root,'.vscode','mcp.json')];
        for(const file of files)await fs.mkdir(path.dirname(file),{recursive:true});
        await fs.writeFile(files[0],`[mcp_servers.${name}]\ncommand = ${JSON.stringify(command)}\nargs = ${JSON.stringify(args)}\n`);
        await fs.writeFile(files[1],JSON.stringify({mcpServers:{[name]:{command,args,disabled:true}}}));
        await fs.writeFile(files[2],JSON.stringify({servers:{[name]:{command,args,type:'stdio'}}}));
        const documents=new Map<string,any>();
        f.stub.workspace.openTextDocument=async(uri:any)=>{
            const document={uri,isDirty:false,version:1,text:await fs.readFile(uri.fsPath,'utf8'),getText(){return this.text;},positionAt:(offset:number)=>offset,
                async save(){await fs.writeFile(uri.fsPath,this.text);this.isDirty=false;return true;}};
            documents.set(uri.fsPath,document);return document;
        };
        f.stub.workspace.applyEdit=async(edit:any)=>{const document=documents.get(edit.uri.fsPath);document.text=edit.after;document.version++;return true;};
        await f.commands.get('airk.installCli')!();
        for(const file of files){const text=await fs.readFile(file,'utf8');assert.ok(!text.includes('--service-project'),f.calls.join('\n'));assert.ok(text.includes('runtime'));assert.ok(!text.includes('airk_explorer')&&text.includes('--explorer-descriptor'),'installation replaces legacy definitions with one endpoint');}
        assert.equal(JSON.parse(await fs.readFile(files[1],'utf8')).mcpServers.airk.disabled,true);
        assert.equal(f.calls.filter(value=>value.startsWith('Updated ')).length,3);
        assert.ok(f.calls.some(value=>value.includes('Old CLI is locked')),'the underlying cleanup cause remains in the output');
        const warning=f.notices.find(value=>value.message.includes('connection or cleanup issues'));
        assert.ok(warning);assert.deepEqual(warning.actions,['Review details']);
        // Unsaved content must block an installer rewrite.
        const before=`[mcp_servers.${name}]\ncommand = ${JSON.stringify(command)}\nargs = ${JSON.stringify(args)}\n`;
        await fs.writeFile(files[0],before);
        f.stub.workspace.textDocuments=[{uri:{fsPath:files[0]},isDirty:true}];
        await f.commands.get('airk.installCli')!();
        assert.equal(await fs.readFile(files[0],'utf8'),before);
        assert.ok(f.calls.some(value=>value.includes('unsaved edits')));
    }finally{await f.close();}
});

test('startup connection migration respects Windows document line endings for compact JSON',async context=>{
    const f=await serviceTestHost(context);
    try{
        const registration=[...f.registrations.values()][0],key=`airk-${registration.collection}`;
        const command=path.join(f.root,'editor-storage','cli','bin',process.platform==='win32'?'airk.exe':'airk');
        const args=['--root',registration.root,'--data-dir',path.join(f.root,'old-data'),'--collection',registration.collection,'mcp'];
        const destination=path.join(registration.root,'.mcp.json');
        await fs.writeFile(destination,JSON.stringify({mcpServers:{[key]:{command,args,disabled:true},other:{command:'keep-other'}}}));
        let doc:any;
        f.stub.workspace.openTextDocument=async(uri:any)=>doc={uri,eol:2,isDirty:false,version:1,text:await fs.readFile(uri.fsPath,'utf8'),getText(){return this.text;},positionAt:(offset:number)=>offset,
            async save(){await fs.writeFile(uri.fsPath,this.text);this.isDirty=false;return true;}};
        f.stub.workspace.applyEdit=async(edit:any)=>{doc.text=edit.after.replace(/\r?\n/g,'\r\n');doc.version++;doc.isDirty=true;return true;};
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        const saved=await fs.readFile(destination,'utf8'),servers=JSON.parse(saved).mcpServers;
        assert.equal(servers[key],undefined);assert.equal(servers.airk.disabled,true);assert.equal(servers.other.command,'keep-other');
        assert.ok(saved.includes('\r\n'));assert.equal(f.calls.some(line=>line.includes('Could not update claude')),false);
    }finally{await f.close();}
});

// 1.4 has no window scheduler or direct-helper fallback. Native continuation,
// contention and worker failures are qualified by the native service workloads.
test('an unavailable service fails explicitly without a helper or window scheduler fallback',async context=>{
    const f=await serviceTestHost(context);
    try{f.state.down=true;await assert.rejects(f.commands.get('airk.index')!(),/service_unavailable/);assert.equal(f.writes.length,0);
        f.state.down=false;await f.commands.get('airk.index')!();assert.equal(f.writes.filter(value=>value.enqueue).length,1);
    }finally{await f.close();}
});

test('canceling storage review preserves the active native binding and performs no indexing',async context=>{
    const f=await serviceTestHost(context);
    try{const before=await f.api.getServerDefinitions();await f.commands.get('airk.chooseStorage')!();assert.equal(f.writes.length,0);
        const selected=path.join(f.root,'new-data');await fs.mkdir(selected);f.state.selectedFolders=[{scheme:'file',fsPath:selected}];f.state.confirm=false;
        await f.commands.get('airk.chooseStorage')!();assert.equal(f.settings.get('storage.path'),selected);
        const after=await f.api.getServerDefinitions();assert.deepEqual(after.map((value:any)=>value.args),before.map((value:any)=>value.args));assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('untrusted workspaces expose no MCP servers and cannot discover, start or mutate native work',async context=>{
    const f=await serviceTestHost(context,{trusted:false});
    try{assert.deepEqual(await f.api.getServerDefinitions(),[]);assert.equal(f.state.discovered,0);
        for(const name of ['airk.index','airk.reindex','airk.openTools','airk.installCli','airk.setupAgent'])await assert.rejects(f.commands.get(name)!(),/runtime_client_no_longer_authorized/);
        assert.equal(f.state.discovered,0);assert.equal(f.state.ensured,0);assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('disabled native execution starts no AIRK process until the user enables it',async context=>{
    const f=await serviceTestHost(context,{automatic:true,nativeExecution:false});
    const launches:string[]=[];
    try{
        assert.deepEqual(await f.api.getServerDefinitions(),[]);
        assert.equal(f.state.discovered,0);assert.equal(f.state.ensured,0);
        assert.match(f.statusBar.text,/native execution off/);
        for(const name of ['airk.index','airk.reindex','airk.openTools','airk.installCli','airk.status','airk.usage'])
            await assert.rejects(Promise.resolve(f.commands.get(name)!()),/native_execution_disabled/);
        // Workspace changes and refreshes stay inert; low-level spawn points refuse first.
        await f.commands.get('airk.treeRefresh')!().catch(()=>undefined);
        await assert.rejects(runText(process.execPath,['--version']),NativeExecutionDisabled);
        assert.throws(()=>new NativeControlTransport('airk',[],1000,(command)=>{launches.push(command);throw new Error('spawned');}),NativeExecutionDisabled);
        await assert.rejects(McpStdioSession.open({command:'airk',args:[],clientName:'fixture',clientVersion:'0',launch:(command:string)=>{launches.push(command);throw new Error('spawned');}} as any),NativeExecutionDisabled);
        assert.deepEqual(launches,[]);
        assert.equal(f.state.discovered,0);assert.equal(f.state.ensured,0);assert.equal(f.writes.length,0);
        await f.commands.get('airk.enableNativeExecution')!();
        assert.equal(f.settings.get('runtime.nativeExecution'),true);
        await f.commands.get('airk.index')!();
        assert.ok(f.state.discovered>0,'enabling resumes the normal runtime path');
    }finally{setNativeExecutionGate(()=>true);await f.close();}
});

test('MCP definitions use reviewed native storage and user response settings while ignoring workspace overrides',async context=>{
    const f=await serviceTestHost(context);
    try{f.settings.set('mcp.responseMode','full');const definitions=await f.api.getServerDefinitions();assert.equal(definitions.length,1);
        assert.equal(definitions[0].command,f.binding.launcher);assert.equal(definitions[0].env.AIRK_MCP_RESPONSE_MODE,'full');
        assert.ok(definitions[0].args.includes(f.binding.installDir));assert.ok(definitions[0].args.includes([...f.registrations.values()][0].root));
        assert.ok(!definitions[0].args.some((value:string)=>value.includes('untrusted-storage')));assert.equal(f.state.ensured,0);assert.equal(f.writes.length,0);
        f.settings.set('mcp.registrationEnabled',false);assert.deepEqual(await f.api.getServerDefinitions(),[]);
    }finally{await f.close();}
});

test('multi-root Index Now selects the active native binding and metadata refresh does not restart MCP definitions',async context=>{
    const f=await serviceTestHost(context,{roots:2});
    try{const before=await f.api.getServerDefinitions();assert.equal(before.length,1);assert.ok(before[0].args.includes('--no-default-project'));let changed=0;f.providers[0].onDidChangeMcpServerDefinitions(()=>changed++);
        await f.api.getServerDefinitions();assert.equal(changed,0);
        f.stub.window.activeTextEditor={document:{uri:f.folders[1].uri}};const picks=f.state.picks;
        await f.commands.get('airk.index')!();assert.equal(f.state.picks,picks);
        const request=f.writes.find(value=>value.enqueue).enqueue;
        assert.equal(f.registrations.get(request.project).root,f.folders[1].uri.fsPath);
        await f.api.getServerDefinitions();assert.equal(changed,0);assert.equal(f.writes.filter(value=>value.enqueue).length,1);
    }finally{await f.close();}
});

test('native tree commands keep the clicked project when another workspace editor is active and reject stale bindings',async context=>{
    const f=await serviceTestHost(context,{roots:2});
    try{
        const project=[...f.registrations.values()][1];
        const spec={command:f.binding.launcher!,root:project.root,dataDir:project.data_root,collection:project.collection,name:'Clicked tree project',service:{installDir:f.binding.installDir,project:project.binding}};
        f.stub.window.activeTextEditor={document:{uri:f.folders[0].uri}};
        await f.commands.get('airk.treeIndex')!(spec);
        assert.equal(f.writes.find(value=>value.enqueue).enqueue.project,project.binding);
        await f.commands.get('airk.treePause')!(spec);
        assert.equal(f.writes.find(value=>value.pause).pause.project,project.binding);
        await f.commands.get('airk.treeReindex')!(spec);
        assert.deepEqual(f.writes.find(value=>value.plan_create).plan_create.request.selection,{selected:[project.binding]});
        const count=f.writes.length;
        const stale={...spec,service:{...spec.service,project:'ef'.repeat(32)}};
        for(const command of ['airk.treeIndex','airk.treePause','airk.treeReindex'])await assert.rejects(f.commands.get(command)!(stale),/workspace_registration_changed/);
        assert.equal(f.writes.length,count);
    }finally{await f.close();}
});

test('agent setup uses the stable launcher, writes only on request and refuses stale or untrusted guides',async context=>{
    const f=await serviceTestHost(context);
    try{f.state.pick=items=>items.find(item=>item.client==='codex')??items[0];await f.commands.get('airk.setupAgent')!();assert.equal(f.panels.length,1);
        const destination=path.join(f.folders[0].uri.fsPath,'.codex','config.toml');await assert.rejects(fs.stat(destination),{code:'ENOENT'});
        await f.panels[0].send({action:'copy'});assert.match(f.clipboard[0],/runtime/);assert.ok(!f.clipboard[0].includes('--service-project'));
        await f.panels[0].send({action:'createCodex'});const before=await fs.readFile(destination,'utf8');assert.doesNotMatch(before,/service-project/);
        f.stub.workspace.workspaceFolders=[];await f.api.getServerDefinitions();await f.panels[0].send({action:'createCodex'});
        assert.match(f.panels[0].webview.html,/Workspace or storage changed/);assert.equal(await fs.readFile(destination,'utf8'),before);
        f.stub.workspace.isTrusted=false;const copied=f.clipboard.length;await f.panels[0].send({action:'copy'});assert.equal(f.clipboard.length,copied);
    }finally{await f.close();}
});

test('Codex setup consolidates an existing legacy connection through guarded editor saves',async context=>{
    const f=await serviceTestHost(context,{assistance:true});
    try{
        if(process.platform==='win32'){
            for(const registration of f.registrations.values())registration.root=path.toNamespacedPath(registration.root);
            await f.api.getServerDefinitions();
        }
        f.state.pick=items=>items.find(item=>item.client==='codex')??items[0];await f.commands.get('airk.setupAgent')!();
        const panel=f.panels.find(panel=>panel.viewType==='airk.agentSetup')??f.panels[0];
        await panel.send({action:'createCodex'});
        const destination=path.join(f.folders[0].uri.fsPath,'.codex','config.toml');
        const created=await fs.readFile(destination,'utf8');assert.ok(created.includes('[mcp_servers.airk]')&&!created.includes('airk_explorer'));
        const project=[...f.registrations.values()][0];
        const retrievalOnly=`[mcp_servers.airk-${project.collection}]\ncommand = ${JSON.stringify(f.binding.launcher)}\nargs = ${JSON.stringify(['runtime','launch','--install-dir',f.binding.installDir,'--','--root',project.root,'--data-dir',project.data_root,'--collection',project.collection,'--service-project',project.binding,'mcp'])}\n`;await fs.writeFile(destination,retrievalOnly);
        let saved=0;
        const document={uri:{fsPath:destination},isDirty:false,version:1,text:retrievalOnly,getText(){return this.text;},positionAt:(offset:number)=>offset,
            async save(){saved++;await fs.writeFile(destination,this.text);return true;}};
        f.stub.workspace.openTextDocument=async()=>document;
        f.stub.workspace.applyEdit=async(edit:any)=>{document.text=edit.after;document.version++;return true;};
        await panel.send({action:'updateCodex'});
        const updated=await fs.readFile(destination,'utf8');assert.ok(updated.includes('[mcp_servers.airk]')&&updated.includes('--explorer-descriptor')&&!updated.includes('airk_explorer'),panel.webview.html);assert.equal(saved,1);
        await panel.send({action:'updateCodex'});assert.equal(saved,1,'current connections require no repeat edit');
        f.stub.workspace.textDocuments=[{uri:{fsPath:destination},isDirty:true}];await panel.send({action:'updateCodex'});
        assert.match(panel.webview.html,/unsaved/i);assert.equal(saved,1);
    }finally{await f.close();}
});

test('lost indexing acknowledgment preserves the exact intent and read-only reconnect never replays it',async context=>{
    const f=await serviceTestHost(context);
    try{f.state.ackLost=true;await assert.rejects(f.commands.get('airk.index')!(),/service_request_outcome_unknown/);
        const keys=[...f.values.keys()].filter(key=>key.startsWith('airk.service.intent.'));assert.equal(keys.length,1);
        const saved=structuredClone(f.values.get(keys[0]));f.state.ackLost=false;
        await f.api.getServerDefinitions();await f.commands.get('airk.serviceConnect')!();
        assert.equal(f.writes.filter(value=>value.enqueue).length,1);assert.deepEqual(f.values.get(keys[0]),saved);
    }finally{await f.close();}
});
