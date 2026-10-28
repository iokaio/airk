// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { serviceTestHost } from './serviceTestHost';
import { renderServiceDashboard } from '../serviceDashboard';
import { RuntimeHost } from '../serviceBootstrap';
import { ServiceError } from '../serviceClient';

test('an installed update keeps one reload action visible and fences older installation attempts',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        const writes=f.writes.length,progress=f.progressRuns.length,ensures=f.state.ensured;
        f.state.installedVersion='1.4.1';f.extensionsChanged();
        await new Promise(resolve=>setImmediate(resolve));
        const notice=f.notices.at(-1)!;
        assert.match(notice.message,/1.4.1 is installed.*running 1.4.0/);
        assert.deepEqual(notice.actions,['Reload to finish update']);
        assert.equal(f.statusBar.command,'airk.reloadUpdatedExtension');
        assert.match(f.statusBar.text,/Reload to finish update/);
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false,'never reload without the user click');
        f.focus();f.extensionsChanged();await f.refreshFolders();
        assert.equal(f.notices.length,1,'dismissal and refocus do not repeat the toast');
        assert.deepEqual(await f.api.getServerDefinitions(),[],'old host must not create another MCP definition');
        assert.equal(f.writes.length,writes);assert.equal(f.progressRuns.length,progress);assert.equal(f.state.ensured,ensures);
        await f.commands.get('airk.installCli')!();assert.equal(f.progressRuns.length,progress);
        assert.equal(f.calls.some(line=>line.startsWith('AIRK: extension_reload_required')),false);
        await f.commands.get('airk.reloadUpdatedExtension')!();
        assert.equal(f.calls.filter(value=>value==='workbench.action.reloadWindow').length,1);
    }finally{await f.close();}
});

test('a pending update found at activation prevents runtime setup and preserves saved registrations',async context=>{
    const f=await serviceTestHost(context,{automatic:true,installedVersion:'1.4.1'});
    try{
        assert.equal(f.progressRuns.length,0);assert.equal(f.state.ensured,0);assert.equal(f.writes.length,0);
        assert.equal(f.registrations.size,1);assert.equal(f.notices.length,1);
        assert.match(f.statusBar.tooltip,/1.4.1 installed; 1.4.0 running/);
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
    }finally{await f.close();}
});

test('ordinary focus and extension-list changes keep a matching active version quiet',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        f.focus();f.extensionsChanged();await f.commands.get('airk.reloadUpdatedExtension')!();
        assert.equal(f.notices.length,0);assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
        assert.doesNotMatch(f.statusBar.text,/Reload to finish update/);
    }finally{await f.close();}
});

test('failed first project preparation preserves completed runtime installation and quiet reopen',async context=>{
    const f=await serviceTestHost(context,{registered:false});
    try{
        context.mock.method(RuntimeHost.prototype,'reviewProject',async()=>{throw new ServiceError('source_unavailable');});
        f.settings.set('cli.autoInstall',true);
        await f.refreshFolders();
        assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.1.')));
        assert.ok(f.warnings.some(message=>message.startsWith('AIRK is installed. Project preparation needs attention')));
        assert.equal(f.calls.some(message=>message.startsWith('Automatic installation failed:')),false);
        assert.equal(f.calls.some(message=>message.startsWith('Installation did not complete.')),false);
        const progress=f.progressRuns.length,notices=f.warnings.length,retirements=f.calls.filter(message=>message==='previous-runtime-retired').length;
        await f.refreshFolders();
        assert.equal(f.progressRuns.length,progress,'project failure does not repeat setup progress');
        assert.equal(f.calls.filter(message=>message==='previous-runtime-retired').length,retirements);
        assert.equal(f.warnings.length,notices,'unchanged project warning is not repeated');
        assert.equal(f.writes.length,0,'failed registration never invents an index request');
    }finally{await f.close();}
});

test('trusted activation installs and registers all folders once with model/index jobs and no setup prompts',async context=>{
    const f=await serviceTestHost(context,{automatic:true,registered:false,roots:2});
    try{
        assert.equal(f.registrations.size,2);assert.equal(f.state.picks,0);assert.equal(f.warnings.length,0);
        assert.ok(f.calls.includes('previous-runtime-retired'));
        assert.equal(f.notices.length,0,'successful activation needs no notification decision');
        assert.ok(f.calls.some(message=>message.includes('This window is running the updated interface')));
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
        assert.equal(f.progressRuns.length,1);
        assert.match(f.progressRuns[0].title,/Checking AIRK .* installation/);
        assert.doesNotMatch(f.progressRuns[0].title,/setup/);
        assert.ok(f.progressRuns[0].messages.some(value=>value.includes('Checking selected runtime')));
        assert.ok(f.progressRuns[0].messages.some(value=>value.includes('Runtime ready. Project indexing continues in Management.')));
        assert.equal(f.writes.filter(value=>value.reserve_project?.setup_watch===true).length,2);
        assert.equal(f.writes.filter(value=>value.enqueue).length,0,'initial setup is owned by the native registration transaction');
        const servers=await f.api.getServerDefinitions();assert.equal(servers.length,1);assert.ok(servers[0].args.includes('--no-default-project'));assert.ok(servers[0].args.includes('--additional-project'));
        const writes=f.writes.length,retirements=f.calls.filter(value=>value==='previous-runtime-retired').length,ensures=f.state.ensured;
        await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.equal(f.state.ensured,ensures,'healthy reopen uses authenticated health without another native start/ensure');
        assert.equal(f.progressRuns.length,1,'verified reopen must not display installation progress');
        assert.equal(f.calls.filter(value=>value==='previous-runtime-retired').length,retirements,'verified reopen must not reinstall the terminal command or retire runtimes');
        assert.equal(f.notices.length,0,'routine reopen must remain quiet');
        f.state.stopped=true;await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.ok(f.calls.some(value=>value.includes('service_explicitly_stopped')));
    }finally{await f.close();}
});

test('completed activation and explicit repair finish without a reload or completion popup',async context=>{
    const f=await serviceTestHost(context);
    try{
        await f.commands.get('airk.installCli')!();
        assert.equal(f.notices.length,0);
        assert.ok(f.calls.some(message=>message.startsWith('AIRK 1.4.0 ready.')));
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
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
        assert.equal(f.progressRuns[0].title,'Updating AIRK 1.4.1 runtime');
        assert.ok(f.calls.some(value=>value.startsWith('Upgrade paused')));
        await f.commands.get('airk.retryUpdate')!();
        assert.equal(attempts,2);assert.equal(f.binding.version,'1.4.1');
        assert.equal((await f.api.getServerDefinitions()).length,1);
        assert.ok(f.calls.some(value=>value.startsWith('AIRK 1.4.1 ready.')));
        assert.equal(f.notices.some(value=>value.message.startsWith('AIRK 1.4.1 installed.')),false);
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
