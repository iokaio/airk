// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { serviceTestHost } from './serviceTestHost';

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

test('MCP definitions use reviewed native storage and user response settings while ignoring workspace overrides',async context=>{
    const f=await serviceTestHost(context);
    try{f.settings.set('mcp.responseMode','full');const definitions=await f.api.getServerDefinitions();assert.equal(definitions.length,1);
        assert.equal(definitions[0].command,f.binding.launcher);assert.equal(definitions[0].env.AIRK_MCP_RESPONSE_MODE,'full');
        assert.ok(definitions[0].args.includes(f.binding.installDir));assert.ok(definitions[0].args.includes([...f.registrations.keys()][0]));
        assert.ok(!definitions[0].args.some((value:string)=>value.includes('untrusted-storage')));assert.equal(f.state.ensured,0);assert.equal(f.writes.length,0);
        f.settings.set('mcp.registrationEnabled',false);assert.deepEqual(await f.api.getServerDefinitions(),[]);
    }finally{await f.close();}
});

test('multi-root Index Now selects the active native binding and metadata refresh does not restart MCP definitions',async context=>{
    const f=await serviceTestHost(context,{roots:2});
    try{const before=await f.api.getServerDefinitions();assert.equal(before.length,2);let changed=0;f.providers[0].onDidChangeMcpServerDefinitions(()=>changed++);
        await f.api.getServerDefinitions();assert.equal(changed,0);
        f.stub.window.activeTextEditor={document:{uri:f.folders[1].uri}};const picks=f.state.picks;
        await f.commands.get('airk.index')!();assert.equal(f.state.picks,picks);
        const request=f.writes.find(value=>value.enqueue).enqueue;
        assert.equal(f.registrations.get(request.project).root,f.folders[1].uri.fsPath);
        await f.api.getServerDefinitions();assert.equal(changed,0);assert.equal(f.writes.filter(value=>value.enqueue).length,1);
    }finally{await f.close();}
});

test('agent setup uses the stable launcher, writes only on request and refuses stale or untrusted guides',async context=>{
    const f=await serviceTestHost(context);
    try{f.state.pick=items=>items.find(item=>item.client==='codex')??items[0];await f.commands.get('airk.setupAgent')!();assert.equal(f.panels.length,1);
        const destination=path.join(f.folders[0].uri.fsPath,'.codex','config.toml');await assert.rejects(fs.stat(destination),{code:'ENOENT'});
        await f.panels[0].send({action:'copy'});assert.match(f.clipboard[0],/runtime/);assert.ok(f.clipboard[0].includes([...f.registrations.keys()][0]));
        await f.panels[0].send({action:'createCodex'});const before=await fs.readFile(destination,'utf8');assert.match(before,/service-project/);
        f.stub.workspace.workspaceFolders=[];await f.api.getServerDefinitions();await f.panels[0].send({action:'createCodex'});
        assert.match(f.panels[0].webview.html,/Workspace or storage changed/);assert.equal(await fs.readFile(destination,'utf8'),before);
        f.stub.workspace.isTrusted=false;const copied=f.clipboard.length;await f.panels[0].send({action:'copy'});assert.equal(f.clipboard.length,copied);
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
