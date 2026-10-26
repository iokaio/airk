// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { serviceTestHost } from './serviceTestHost';
import { renderServiceDashboard } from '../serviceDashboard';
import { RuntimeHost } from '../serviceBootstrap';

test('trusted activation installs and registers all folders once with model/index jobs and no setup prompts',async context=>{
    const f=await serviceTestHost(context,{automatic:true,registered:false,roots:2});
    try{
        assert.equal(f.registrations.size,2);assert.equal(f.state.picks,0);assert.equal(f.warnings.length,0);
        assert.ok(f.calls.includes('previous-runtime-retired'));
        assert.equal(f.notices.length,1);
        assert.deepEqual(f.notices[0].actions,['MCP Servers','Reload Window','Show Details']);
        assert.match(f.notices[0].message,/Copilot/);
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
        assert.equal(f.progressRuns.length,1);
        assert.match(f.progressRuns[0].title,/AIRK .* setup/);
        assert.ok(f.progressRuns[0].messages.some(value=>value.includes('Checking runtime files')));
        assert.ok(f.progressRuns[0].messages.some(value=>value.includes('Installation complete')));
        assert.equal(f.writes.filter(value=>value.configure_watch?.enabled===true).length,2);
        assert.deepEqual(f.writes.filter(value=>value.enqueue).map(value=>value.enqueue.kind),['model_setup','index','model_setup','index']);
        assert.equal((await f.api.getServerDefinitions()).length,2);
        const writes=f.writes.length;await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.equal(f.notices.length,1,'routine reopen must not repeat the install notice');
        f.state.stopped=true;await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.ok(f.calls.some(value=>value.includes('service_explicitly_stopped')));
    }finally{await f.close();}
});

test('installation notification opens MCP controls, reloads only on click, and exposes restart details',async context=>{
    const f=await serviceTestHost(context);
    try{
        for(const [action,expected] of [['MCP Servers','workbench.mcp.listServer'],['Reload Window','workbench.action.reloadWindow'],['Show Details','output-shown']]){
            f.state.noticeAction=action;
            await f.commands.get('airk.installCli')!();
            await new Promise(resolve=>setImmediate(resolve));
            assert.ok(f.calls.includes(expected));
        }
        assert.equal(f.notices.length,3,'one notice per explicit repair');
        assert.ok(f.calls.some(value=>value.includes('fully exit all VS Code windows')));
        assert.match(renderServiceDashboard(true,1,1),/command:airk.manageVscodeMcp/);
        assert.match(renderServiceDashboard(true,1,1),/Restart external Codex and Claude Code sessions separately/);
    }finally{await f.close();}
});

test('a launcher held by clients offers retry and does not report successful activation',async context=>{
    const f=await serviceTestHost(context,{upgrade:true});
    try{
        let attempts=0;
        context.mock.method(RuntimeHost.prototype,'transition',async()=>{
            attempts++;
            if(attempts===1)throw new Error('airk_client_restart_required: launcher is in use');
            f.binding.version='1.4.1';return {};
        });
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        assert.equal(f.binding.version,'1.4.0');assert.equal(f.writes.length,0);
        const notice=f.notices.find(value=>value.message.includes('waiting for connected tools'));
        assert.ok(notice);assert.deepEqual(notice.actions,['MCP Servers','Retry Update','Show Details']);
        assert.equal(f.notices.some(value=>value.message.startsWith('AIRK 1.4.1 installed.')),false);
        assert.ok(f.progressRuns[0].messages.some(value=>value.startsWith('Upgrading 1.4.0 → 1.4.1')));
        assert.ok(f.progressRuns[0].messages.some(value=>value.startsWith('Upgrade paused')));
        await f.commands.get('airk.retryUpdate')!();
        assert.equal(attempts,2);assert.equal(f.binding.version,'1.4.1');
        assert.equal((await f.api.getServerDefinitions()).length,1);
        assert.ok(f.notices.some(value=>value.message.startsWith('AIRK 1.4.1 installed.')));
    }finally{await f.close();}
});

test('untrusted activation does not install or register and dashboard explains prerequisites and storage changes',async context=>{
    const f=await serviceTestHost(context,{automatic:true,registered:false,trusted:false});
    try{assert.equal(f.state.ensured,0);assert.equal(f.registrations.size,0);assert.equal(f.writes.length,0);}
    finally{await f.close();}
    assert.match(renderServiceDashboard(false,1,0),/Trust this workspace/);
    assert.match(renderServiceDashboard(true,0,0),/Open a project folder/);
    const html=renderServiceDashboard(true,1,1);
    assert.match(html,/1 workspace project is registered/);assert.match(html,/command:airk.chooseStorage/);
    assert.match(html,/Job acceptance means work is queued/);
});

test('1.4 patch upgrade retains existing storage bindings and does not reset watching or enqueue a fresh index',async context=>{
    const f=await serviceTestHost(context,{automatic:true,upgrade:true});
    try{
        assert.ok(f.calls.includes('upgrade-reviewed'));assert.ok(f.calls.includes('upgrade-applied'));
        assert.ok(f.calls.includes('previous-runtime-retired'));
        assert.equal(f.registrations.size,1);assert.equal(f.writes.length,0);
        const definitions=await f.api.getServerDefinitions();assert.equal(definitions.length,1);
        assert.ok(definitions[0].args.includes([...f.registrations.keys()][0]));
        const saved=[...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.1.'));
        await f.refreshFolders();assert.deepEqual([...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.1.')),saved);
        assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('automatic reopen keeps a user-selected replacement index location and manual watching',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        const moved=require('node:path').join(f.root,'moved-index');
        f.state.selectedFolders=[{fsPath:moved}];await f.commands.get('airk.chooseStorage')!();
        f.state.pick=items=>items.find(item=>item.enabled===false)??items[0];
        await f.commands.get('airk.indexMode')!();
        const saved=[...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.'));
        const writes=f.writes.length;await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.deepEqual([...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.')),saved);
        assert.equal(f.settings.get('storage.path'),moved);
        assert.ok(f.writes.some(value=>value.configure_watch?.enabled===false));
    }finally{await f.close();}
});
