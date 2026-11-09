// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import {createHash} from 'node:crypto';
import * as globalInventory from '../globalAgentInventory';
const realGlobalInventory=globalInventory.inventoryGlobalAgentConfigs;
import * as fs from 'node:fs/promises';
import { serviceTestHost } from './serviceTestHost';
import { renderServiceDashboard } from '../serviceDashboard';
import { RuntimeHost, legacyOriginLocatorIdentity, type LegacyOriginLocator } from '../serviceBootstrap';
import { ServiceWorkspace } from '../serviceWorkspace';
import { ServiceError } from '../serviceClient';
import { httpConfigurationConnectionId, refreshHttpAgentConfig } from '../agentMigration';
import { httpAuthorityDigest } from '../serviceHttp';
import * as path from 'node:path';

async function startupState(f:Awaited<ReturnType<typeof serviceTestHost>>,requestId:string){
    await f.commands.get('airk.explorer.open')!();
    const panel=f.panels.find(panel=>panel.viewType==='airk.explorer');assert.ok(panel);
    await panel.send({v:1,type:'ready',requestId});
    return panel.posted.filter((entry:any)=>entry.type==='state').at(-1)?.state.startup;
}

test('successful explicit setup clears an earlier failed startup without creating indexes',async context=>{
    let release!:()=>void;const discoveryGate=new Promise<void>(resolve=>{release=resolve;});
    const f=await serviceTestHost(context,{automatic:true,deferStartup:true,discoveryGate,registered:false});
    try{
        f.state.down=true;release();await f.api.whenReady();
        assert.equal((await startupState(f,'failed-startup'))?.failed,true);
        const failures=f.calls.filter(value=>value.startsWith('Automatic installation failed:')).length;assert.ok(failures>0);
        f.state.down=false;await f.commands.get('airk.setUp')!();
        assert.equal(await startupState(f,'successful-retry'),undefined);
        assert.equal(f.calls.filter(value=>value.startsWith('Automatic installation failed:')).length,failures,'recorded failures remain in output');
        assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project||value.enqueue).length,0);
    }finally{release();await f.close();}
});

test('failed setup stays visible through missing-runtime refresh until successful setup',async context=>{
    const f=await serviceTestHost(context,{automatic:false,fresh:true,registered:false});
    const discover=RuntimeHost.prototype.discover;
    context.mock.method(RuntimeHost.prototype,'discover',async function(this:RuntimeHost){
        const value=await discover.call(this);
        return f.state.down?{...value,installed:false,inspection:undefined}:value;
    });
    try{
        f.state.down=true;await f.commands.get('airk.setUp')!();
        assert.match(f.statusBar.text,/setup failed/);
        await f.refreshFolders();assert.match(f.statusBar.text,/setup failed/,'refresh must not replace failure with neutral setup text');
        f.state.down=false;await f.commands.get('airk.setUp')!();
        assert.doesNotMatch(f.statusBar.text,/setup failed/);
        f.state.down=true;await f.commands.get('airk.setUp')!();assert.match(f.statusBar.text,/setup failed/);
        f.state.installedVersion='1.4.1';f.extensionsChanged();await new Promise(resolve=>setImmediate(resolve));
        await f.refreshFolders();assert.match(f.statusBar.text,/Reload to finish update/,'pending interface update keeps priority over setup failure');
    }finally{await f.close();}
});

test('upgrade stays incomplete until metadata, migration and launcher removal can finish',async context=>{
    let release!:()=>void;const discoveryGate=new Promise<void>(resolve=>{release=resolve;});
    const f=await serviceTestHost(context,{automatic:true,deferStartup:true,discoveryGate,registered:false});
    const preparation=context.mock.method(ServiceWorkspace.prototype,'reconcileStartup',async()=>{throw new ServiceError('project_metadata_unavailable');});
    try{
        release();await f.api.whenReady();assert.equal((await startupState(f,'preparation-failed'))?.failed,true);
        assert.ok(f.calls.some(value=>value.includes('upgrade completion blocked')));
        await f.commands.get('airk.setUp')!();
        assert.equal((await startupState(f,'retry-still-failed'))?.failed,true,'runtime health cannot erase unresolved preparation failure');
        preparation.mock.restore();const ensures=f.state.ensured,progress=f.progressRuns.length;
        await f.refreshFolders();
        assert.ok(f.calls.includes('legacy-launchers-retired'));
        assert.equal(await startupState(f,'healthy-reconnect'),undefined);
        assert.ok(f.state.ensured>ensures);assert.ok(f.progressRuns.length>progress,'an incomplete upgrade reruns its completion gates');
        assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project||value.enqueue).length,0);
    }finally{release();await f.close();}
});

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

test('runtime installation never calls legacy project preparation for missing indexes and keeps reopen quiet',async context=>{
    const f=await serviceTestHost(context,{registered:false});
    try{
        let reviewed=0;context.mock.method(RuntimeHost.prototype,'reviewProject',async()=>{reviewed++;throw new ServiceError('source_unavailable');});
        f.settings.set('cli.autoInstall',true);
        await f.refreshFolders();
        assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')));
        assert.equal(reviewed,0);assert.equal(f.registrations.size,0);assert.equal(f.warnings.length,0);
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

test('trusted activation installs the runtime once without registering or indexing open folders',async context=>{
    const f=await serviceTestHost(context,{automatic:true,registered:false,roots:2});
    try{
        assert.equal(f.registrations.size,0);assert.equal(f.state.picks,0);assert.equal(f.warnings.length,0);
        assert.ok(f.calls.includes('previous-runtime-retired'));
        assert.equal(f.notices.length,0,'successful activation needs no notification decision');
        assert.ok(f.calls.some(message=>message.includes('This window is running the updated interface')));
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
        assert.equal(f.progressRuns.length,1);
        assert.match(f.progressRuns[0].title,/Checking AIRK .* installation/);
        assert.doesNotMatch(f.progressRuns[0].title,/setup/);
        assert.ok(f.progressRuns[0].messages.some(value=>value.includes('Checking selected runtime')));
        assert.ok(f.progressRuns[0].messages.some(value=>value.includes('Runtime ready. Set up new indexes explicitly in Indexing')));
        assert.equal(f.writes.filter(value=>value.reserve_project).length,0);assert.equal(f.writes.filter(value=>value.enqueue).length,0);
        const servers=await f.api.getServerDefinitions();assert.deepEqual(servers,[],'missing indexes and grants cannot produce MCP definitions');
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
        assert.equal(definitions[0].uri.toString(),f.httpUrl);const enrolled=f.state.httpConnections.at(-1)!.selection;assert.equal(enrolled.kind,'imported_legacy');if(enrolled.kind==='imported_legacy')assert.equal(enrolled.selection.registration.root,[...f.registrations.values()][0].root);
        const saved=[...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.1.'));
        await f.refreshFolders();assert.deepEqual([...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.1.')),saved);
        assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

// Follow the shipped manifest so a release bump cannot silently leave its upgrade path disabled.
const releaseVersion:string=require('../../package.json').version;
test('fresh native registry does not recreate an index from surviving editor receipts',async context=>{
    const f=await serviceTestHost(context,{automatic:true,runtimeVersion:releaseVersion,bundledVersion:releaseVersion});
    try{
        f.registrations.clear(); // User deleted Local/Roaming AIRK state; VS Code's Memento survived.
        const writes=f.writes.length;
        await f.refreshFolders();
        assert.equal(f.registrations.size,0);assert.equal(f.writes.length,writes);assert.deepEqual(await f.api.getServerDefinitions(),[]);
        const again=f.writes.length;await f.refreshFolders();assert.equal(f.writes.length,again);
        assert.ok(f.calls.some(value=>value.includes('Indexing → Index library')));assert.ok(!f.calls.some(value=>value.includes('Restoring workspace registration')));
    }finally{await f.close();}
});
test('startup honors a native removed project instead of recreating it from editor receipts',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        for(const [key,value] of f.registrations)f.inactiveRegistrations.set(key,value);
        f.registrations.clear();const writes=f.writes.length;
        await f.refreshFolders();
        assert.equal(f.registrations.size,0);assert.equal(f.writes.length,writes);
        assert.equal((await f.api.getServerDefinitions()).length,0);
        await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.ok(f.calls.some(value=>value.startsWith('Index remains removed:')));
    }finally{await f.close();}
});
test('startup adopts an existing manually restored index for the same root without resetting it',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        const previous=[...f.registrations.values()][0],restored={...previous,binding:'cd'.repeat(32),collection:'manually-restored',data_root:path.join(f.root,'restored-index')};
        f.registrations.clear();f.registrations.set(restored.binding,restored);const writes=f.writes.length;
        await f.refreshFolders();assert.equal(f.registrations.size,1);assert.equal(f.writes.length,writes);
        await assert.rejects(f.api.getServerDefinitions(),/legacy_selection_review_required/,'a newly adopted different Index cannot inherit old provider authority');
        const again=f.writes.length;await f.refreshFolders();assert.equal(f.writes.length,again);
    }finally{await f.close();}
});
test('an unavailable registry never triggers automatic re-registration',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        f.registrations.clear();f.state.down=true;const writes=f.writes.length;
        await f.refreshFolders();assert.equal(f.registrations.size,0);assert.equal(f.writes.length,writes);
        assert.ok(f.calls.some(value=>value.includes('service_unavailable')));
    }finally{await f.close();}
});
for(const from of ['1.4.4','1.5.12','1.6.0','1.8.7','1.8.8']){
    const to=releaseVersion;
    test(`activation upgrades ${from} to ${to} before connecting and reopens quietly`,async context=>{
        const f=await serviceTestHost(context,{automatic:true,runtimeVersion:from,bundledVersion:to});
        try{
            assert.equal(f.binding.version,to);
            assert.equal(f.calls.filter(value=>value==='upgrade-applied').length,1);
            assert.ok(f.calls.indexOf('upgrade-applied')<f.calls.indexOf('Checking selected runtime…'));
            assert.equal(f.calls.some(value=>value.includes('explicit_runtime_upgrade_or_matching_extension_required')),false);
            assert.equal(f.calls.some(value=>value.startsWith('Automatic installation failed:')),false);
            assert.equal(f.values.get(`airk.installComplete.2.${f.binding.installDir}`),`${to}:${f.binding.runtimeSha256}`);
            assert.equal(f.registrations.size,1);assert.equal(f.writes.length,0);
            assert.match(f.progressRuns[0].title,/Updating AIRK/);
            const registrations=structuredClone([...f.registrations]),ensures=f.state.ensured;
            await f.refreshFolders();
            assert.deepEqual([...f.registrations],registrations);
            assert.equal(f.state.ensured,ensures);assert.equal(f.progressRuns.length,1);
            assert.equal(f.calls.filter(value=>value==='upgrade-applied').length,1);
        }finally{await f.close();}
    });
}
test('unsupported automatic downgrade remains visible and never applies a transition',async context=>{
    const [major,minor,patch]=releaseVersion.split('.').map(Number);
    const newerRuntime=`${major}.${minor}.${patch+1}`;
    const f=await serviceTestHost(context,{automatic:true,runtimeVersion:newerRuntime,bundledVersion:releaseVersion});
    try{
        assert.equal(f.binding.version,newerRuntime);
        assert.ok(!f.calls.includes('upgrade-applied'));
        assert.ok(f.calls.some(line=>line.startsWith('Automatic installation failed:')&&line.includes('explicit_runtime_upgrade_or_matching_extension_required')));
        assert.ok(f.calls.some(line=>line.startsWith('AIRK installation needs attention:')));
    }finally{await f.close();}
});

test('failed supported automatic upgrade is reported by its setup owner',async context=>{
    const f=await serviceTestHost(context,{automatic:false,runtimeVersion:'1.6.0',bundledVersion:releaseVersion});
    try{
        context.mock.method(RuntimeHost.prototype,'transition',async()=>{throw new ServiceError('upgrade_fixture_refused');});
        const before=f.calls.length;
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        assert.equal(f.binding.version,'1.6.0');assert.ok(!f.calls.includes('upgrade-applied'));
        assert.ok(f.calls.slice(before).some(line=>line.startsWith('Automatic installation failed:')&&line.includes('upgrade_fixture_refused')));
        assert.ok(f.calls.slice(before).some(line=>line.startsWith('AIRK installation needs attention:')));
    }finally{await f.close();}
});

for(const command of ['airk.installCli','airk.retryUpdate']){
    test(`${command} upgrades a completed older installation to the shipped version`,async context=>{
        const f=await serviceTestHost(context,{runtimeVersion:'1.5.12',bundledVersion:releaseVersion});
        try{
            const key=`airk.installComplete.2.${f.binding.installDir}`;
            f.values.set(key,`1.5.12:${f.binding.runtimeSha256}`);
            const start=f.calls.length,registrations=structuredClone([...f.registrations]);
            await f.commands.get(command)!();
            assert.equal(f.binding.version,releaseVersion);
            assert.equal(f.values.get(key),`${releaseVersion}:${f.binding.runtimeSha256}`);
            assert.equal(f.calls.slice(start).some(value=>value.includes('explicit_runtime_upgrade_or_matching_extension_required')),false);
            assert.deepEqual([...f.registrations],registrations);assert.equal(f.writes.length,0);
        }finally{await f.close();}
    });
}

test('storage command opens explicit setup without changing the saved location or existing watching',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        const moved=require('node:path').join(f.root,'moved-index'),storage=f.settings.get('storage.path');
        f.state.selectedFolders=[{fsPath:moved}];await f.commands.get('airk.chooseStorage')!();
        f.state.pick=items=>items.find(item=>item.enabled===false)??items[0];
        await f.commands.get('airk.indexMode')!();
        const saved=[...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.'));
        const writes=f.writes.length;await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.deepEqual([...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.')),saved);
        assert.equal(f.settings.get('storage.path'),storage);assert.ok(f.calls.some(value=>value.includes('Set up an index in Indexing')));
        assert.ok(f.writes.some(value=>value.configure_watch?.enabled===false));
    }finally{await f.close();}
});


test('upgrade preserves missing folders as unregistered until an explicit native index setup',async context=>{
    const f=await serviceTestHost(context,{automatic:true,registered:false,roots:2,runtimeVersion:'1.6.0',bundledVersion:releaseVersion});
    try{
        assert.equal(f.binding.version,releaseVersion);
        assert.equal(f.registrations.size,0);
        assert.equal(f.state.picks,0);
        assert.equal(f.notices.length,0);
        assert.equal(f.warnings.length,0);
        const reservations=f.writes.filter(value=>value.reserve_project);
        assert.equal(reservations.length,0);
        const writes=f.writes.length;await f.refreshFolders();assert.equal(f.writes.length,writes);
    }finally{await f.close();}
});


test('a deleted open folder does not trigger registration of the remaining folders',async context=>{
    const fs=await import('node:fs/promises');
    const f=await serviceTestHost(context,{registered:false,roots:2});
    try{
        const missing=f.folders[0].uri.fsPath;
        const held=path.join(f.root,'moved-folder');
        await fs.rename(missing,held);
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        assert.equal(f.registrations.size,0);
        assert.equal(f.warnings.length,0);
        assert.equal(f.state.picks,0);
        assert.ok(f.calls.some(value=>value.startsWith('Source folder is missing:')));
        assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
    }finally{await f.close();}
});

test('transient selected metadata failure never authorizes creating missing indexes',async context=>{
    const f=await serviceTestHost(context,{registered:false,roots:2});
    try{
        context.mock.method(RuntimeHost.prototype as any,'invoke',async(_command:string,args:string[])=>{
            assert.deepEqual(args.slice(0,2),['runtime','source-scope']);
            throw new ServiceError('selected_metadata_fixture_unavailable');
        });
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        assert.equal(f.registrations.size,0);
        assert.equal(f.warnings.length,0);
        assert.ok(f.calls.some(line=>line.includes('Selected project availability is unconfirmed:')&&line.includes('selected_metadata_fixture_unavailable')));
        assert.ok(!f.calls.some(line=>line.includes('Project preparation needs attention:')));
        assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
    }finally{await f.close();}
});

test('first installation waits for review: dismiss and defer change nothing persistent until explicit setup',async context=>{
    const f=await serviceTestHost(context,{fresh:true,registered:false});
    try{
        f.settings.set('cli.autoInstall',true);
        await f.refreshFolders();
        const reviews=()=>f.notices.filter(notice=>notice.message==='Set up AIRK on this computer?').length;
        assert.equal(reviews(),1);
        assert.equal(f.state.ensured,0);assert.equal(f.registrations.size,0);
        assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
        f.state.noticeAction='Not now';await f.refreshFolders();
        assert.equal(reviews(),2);assert.equal(f.state.ensured,0);
        await f.refreshFolders();
        assert.equal(reviews(),2,'a saved deferral is honored without asking again');
        assert.equal(f.state.ensured,0);assert.equal(f.registrations.size,0);
        await f.commands.get('airk.setUp')!();
        assert.ok(f.state.ensured>0);assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
    }finally{await f.close();}
});

test('first installation review can disable automatic setup or accept it once',async context=>{
    const f=await serviceTestHost(context,{fresh:true,registered:false});
    try{
        f.settings.set('cli.autoInstall',true);f.state.noticeAction="Don't set up automatically";
        await f.refreshFolders();
        assert.equal(f.settings.get('cli.autoInstall'),false);assert.equal(f.state.ensured,0);
        f.settings.set('cli.autoInstall',true);f.state.noticeAction='Set up';
        await f.refreshFolders();
        assert.ok(f.state.ensured>0);assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
        const reviews=f.notices.length;await f.refreshFolders();
        assert.equal(f.notices.length,reviews,'accepted setup is not reviewed again');
    }finally{await f.close();}
});

test('customized first installation offers shell integration without folder registration choices',async context=>{
    const f=await serviceTestHost(context,{fresh:true,registered:false,roots:2});
    try{
        f.settings.set('cli.autoInstall',true);f.state.noticeAction='Customize…';
        let offered:any[]=[];
        f.state.pick=items=>{offered=items;return [];};
        await f.refreshFolders();
        assert.equal(offered.length,1);assert.equal(offered[0].label,'Terminal command in external shells');assert.equal(offered.some(item=>item.folder),false);
        assert.equal(f.settings.get('cli.shellIntegration'),'off');
        assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
    }finally{await f.close();}
});

test('home folders and ordinary folders both remain unregistered after runtime setup',async context=>{
    const os=require('node:os') as typeof import('node:os');
    const f=await serviceTestHost(context,{fresh:true,registered:false,roots:2});
    try{
        context.mock.method(os,'homedir',()=>f.folders[0].uri.fsPath);
        f.settings.set('cli.autoInstall',true);f.state.noticeAction='Set up';
        await f.refreshFolders();
        assert.ok(f.notices.some(notice=>notice.message==='Set up AIRK on this computer?'));
        assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
        assert.ok(f.calls.some(line=>line.includes('Opening folders does not create indexes')));
    }finally{await f.close();}
});

test('an uninstall that never wrote a final result is reported as interrupted, never as success',async context=>{
    const fs=await import('node:fs/promises');
    const f=await serviceTestHost(context);
    try{
        const directory=path.join(f.root,'native-state','extension-uninstall');await fs.mkdir(directory,{recursive:true});
        await fs.writeFile(path.join(directory,'last-result.json'),JSON.stringify({schema:1,state:'started',startedAt:'2026-10-30T00:00:00Z',pid:2147483646,job:'/tmp/airk-uninstall-fixture'}));
        await f.commands.get('airk.showLastUninstallResult')!();
        assert.ok(f.calls.some(line=>line.includes('interrupted or unknown')),f.calls.join('\n'));
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        assert.ok(f.warnings.some(message=>message.includes('did not finish cleanly')));
    }finally{await f.close();}
});

test('data review is read-only until a plan digest is confirmed and never deletes project indexes directly',async context=>{
    const bootstrap=require('../serviceBootstrap') as typeof import('../serviceBootstrap');
    const f=await serviceTestHost(context);
    const calls:string[][]=[];
    context.mock.method(bootstrap,'nativeRunner',async(_main:string,args:string[])=>{
        calls.push(args);
        if(args[1]==='inventory')return {protocol:1,incomplete:false,not_covered:['exports'],projects:[{collection:'fixture',root:'/src',data_root:'/data',storage_warning:'storage_not_private: fixture'}],
            locations:[{id:'stable:logs',kind:'logs',path:'/logs',exists:true,bytes:1048576,complete:true,purge:'purgeable',consequence:'Deletes logs.'},
                {id:'stable:data',kind:'data',path:'/data',exists:true,bytes:1,complete:true,purge:'requires_project_removal',consequence:'Indexes.'}]};
        if(args.includes('--confirm'))return {protocol:1,purged:[{id:'stable:logs',path:'/logs',files_deleted:3}],retained:[],complete:true};
        return {protocol:1,plan:[{id:'stable:logs',path:'/logs',bytes:1048576,consequence:'Deletes logs.'}],plan_digest:'ab'.repeat(32)};
    });
    try{
        let offered:any[]=[];
        f.state.pick=items=>{offered=items;return items.filter((item:any)=>item.label==='stable:logs'||item.label==='stable:data');};
        f.state.confirm=false;
        await f.commands.get('airk.reviewData')!();
        assert.deepEqual(calls.map(args=>args.includes('--confirm')),[false,false],'declining the confirmation deletes nothing');
        assert.ok(f.calls.some(line=>line.includes('Not removable here')));
        assert.ok(offered.some((item:any)=>item.project&&String(item.detail).includes('other accounts may read')),'a storage privacy warning is shown');
        f.state.confirm=true;calls.length=0;
        await f.commands.get('airk.reviewData')!();
        assert.deepEqual(calls.at(-1)!.slice(-2),['--confirm','ab'.repeat(32)]);
        assert.ok(f.calls.some(line=>line.includes('Deleted 3 files')));
    }finally{await f.close();}
});

test('Windows extension-path casing aliases keep the same loaded code active',async context=>{
    if(process.platform!=='win32'){context.skip('Windows path identity');return;}
    const f=await serviceTestHost(context,{codeFile:true,automatic:true});
    try{
        for(const extensionPath of [f.root.toLowerCase(),f.root.toUpperCase(),path.toNamespacedPath(f.root.toUpperCase())]){
            f.stub.extensions.getExtension=()=>({extensionPath,packageJSON:{version:f.state.installedVersion}});
            f.focus();f.extensionsChanged();
            await new Promise(resolve=>setImmediate(resolve));
            assert.equal(f.notices.length,0,extensionPath);
            assert.doesNotMatch(f.statusBar.text,/Reload to finish update/,extensionPath);
            assert.notEqual((await f.api.getServerDefinitions()).length,0,'matching code retains native MCP admission');
        }
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
    }finally{await f.close();}
});

for(const change of ['size','mtime'] as const)test(`a same-version VSIX ${change} change still offers reload and fences older code`,async context=>{
    const fs=await import('node:fs/promises');
    const f=await serviceTestHost(context,{codeFile:true,automatic:true});
    try{
        f.focus();f.extensionsChanged();
        assert.equal(f.notices.some(notice=>notice.actions.includes('Reload to finish update')),false,'an unchanged installation stays quiet');
        const file=path.join(f.root,'out','serviceExtension.js'),before=await fs.stat(file);
        if(change==='size'){
            await fs.writeFile(file,'// fixture build 2 with different bytes\n');
            assert.notEqual((await fs.stat(file)).size,before.size);
        }else{
            await fs.utimes(file,before.atime,new Date(before.mtimeMs+5000));
            const after=await fs.stat(file);
            assert.equal(after.size,before.size,'mtime-only change retains identical file size');
            assert.notEqual(after.mtimeMs,before.mtimeMs);
        }
        f.focus();f.extensionsChanged();
        assert.ok(f.notices.some(notice=>notice.message.includes('(rebuilt) is installed')&&notice.actions.includes('Reload to finish update')));
        assert.match(f.statusBar.text,/Reload to finish update/);
        assert.deepEqual(await f.api.getServerDefinitions(),[],'changed code cannot admit native MCP through the older interface');
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false,'reload remains an explicit user action');
    }finally{await f.close();}
});

for(const asset of ['explorerWebview.js','explorerWebview.css','workbenchWebview.js','workbenchWebview.css'])test(`a same-version ${asset} replacement updates the open Explorer without refocusing`,async context=>{
    const fs=await import('node:fs/promises');
    const f=await serviceTestHost(context,{codeFile:true,automatic:true});
    try{
        await f.commands.get('airk.explorer.open')!();
        const panel=f.panels.find(panel=>panel.viewType==='airk.explorer');assert.ok(panel);
        await panel.send({v:1,type:'ready',requestId:'asset-ready'});
        const hostFile=path.join(f.root,'out','serviceExtension.js'),before=await fs.stat(hostFile);
        f.assetChanged();assert.equal(f.notices.length,0,'unchanged assets stay quiet');
        await fs.writeFile(path.join(f.root,'out',asset),'// replacement UI with changed bytes\n');
        f.assetChanged();f.assetCreated();
        await new Promise(resolve=>setImmediate(resolve));
        assert.equal((await fs.stat(hostFile)).mtimeMs,before.mtimeMs,'host code remains unchanged');
        assert.equal(f.notices.length,1,'repeated asset events offer one reload');
        assert.match(f.statusBar.text,/Reload to finish update/);
        const state=panel.posted.filter((entry:any)=>entry.type==='state').at(-1)?.state;
        assert.deepEqual(state.extensionUpdate,{runningVersion:'1.4.0',installedVersion:'1.4.0 (rebuilt)'});
        assert.equal(f.panels.filter(panel=>panel.viewType==='airk.explorer').length,1,'the existing panel receives the update');
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false,'replacement never reloads without a click');
        assert.deepEqual(await f.api.getServerDefinitions(),[],'old host is fenced after UI replacement');
    }finally{await f.close();}
    assert.ok(f.assetWatchers.every(watcher=>watcher.disposed),'extension asset watchers are disposed');
});

test('an incomplete asset replacement waits for the complete payload before offering reload',async context=>{
    const fs=await import('node:fs/promises');
    const f=await serviceTestHost(context,{codeFile:true,automatic:true});
    try{
        const file=path.join(f.root,'out','explorerWebview.css'),original=await fs.readFile(file);
        await fs.unlink(file);f.assetChanged();
        assert.equal(f.notices.length,0,'temporarily missing assets do not label a rebuild');
        await fs.writeFile(file,Buffer.concat([original,Buffer.from('/* replacement */')]));f.assetCreated();
        assert.equal(f.notices.length,1,'creation of the complete replacement is observed');
    }finally{await f.close();}
});


for(const [kind,roots,workspaceFile] of [['folder',1,false],['single-root workspace',1,true],['multi-root workspace',2,true]] as const){
    test(`opening a new unindexed ${kind} never creates or maintains an index`,async context=>{
        const f=await serviceTestHost(context,{automatic:true,registered:false,roots,workspaceFile});
        try{
            assert.equal(f.folders.length,roots);
            assert.equal(Boolean(f.stub.workspace.workspaceFile),workspaceFile,'activation receives a saved workspace file for workspace cases');
            if(workspaceFile)assert.match(f.stub.workspace.workspaceFile.fsPath,/\.code-workspace$/);
            assert.ok(f.state.ensured>0,'runtime setup can complete independently of index creation');
            const untouched=()=>{
                assert.equal(f.registrations.size,0,'opening sources cannot reserve a native index');
                assert.equal(f.writes.length,0,'opening sources cannot enqueue work, configure watching or plan index work');
                assert.deepEqual(f.nativeMethods.filter(name=>/^(reserve_project|enqueue|configure_watch|catalog_prepare|catalog_apply|catalog_index_setup_apply|catalog_index_setup_draft)$/.test(name)),[],'no index-creation or source-maintenance transport is called');
                assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.service.workspace.1.')),false,'no editor registration is invented');
            };
            untouched();
            assert.deepEqual(await f.api.getServerDefinitions(),[],'no automatic MCP evidence scope is fabricated');
            f.focus();f.extensionsChanged();await f.refreshFolders();
            untouched();
            assert.deepEqual(await f.api.getServerDefinitions(),[]);
            assert.ok(f.calls.some(line=>line.includes('Opening folders does not create indexes.')));
        }finally{await f.close();}
    });
}

/** Model the editor's real guarded file/save boundary; disk verification remains production. */
function migrationEditor(f:Awaited<ReturnType<typeof serviceTestHost>>,save=false){
    const documents=new Map<string,any>();
    f.stub.WorkspaceEdit=class{edits:any[]=[];replace(uri:any,_range:any,text:string){this.edits.push({uri,text});}};
    f.stub.Range=class{constructor(..._args:any[]){}};
    f.stub.workspace.openTextDocument=async(uri:any)=>{
        if(typeof uri.content==='string')return uri;
        const filename=uri.fsPath;let document=documents.get(filename);
        if(!document){let text=await fs.readFile(filename,'utf8');document={uri,version:1,eol:1,isDirty:false,getText:()=>text,positionAt:(offset:number)=>offset,
            replace:(next:string)=>{text=next;document.version++;document.isDirty=true;},
            save:async()=>{if(save)return false;await fs.writeFile(filename,text);document.isDirty=false;return true;}};
            documents.set(filename,document);f.stub.workspace.textDocuments.push(document);
        }return document;
    };
    f.stub.workspace.applyEdit=async(edit:any)=>{for(const row of edit.edits){const doc=await f.stub.workspace.openTextDocument(row.uri);doc.replace(row.text);}return true;};
    return documents;
}
function legacyConfiguration(f:Awaited<ReturnType<typeof serviceTestHost>>){
    const registration=[...f.registrations.values()][0],command=path.join(f.binding.installDir,process.platform==='win32'?'airk-launcher.exe':'airk-launcher');
    return {registration,command,configuration:{keep:'unrelated user setting',mcpServers:{['airk-'+registration.collection]:{command,args:['runtime','launch','--install-dir',f.binding.installDir,'--','--root',registration.root,'--data-dir',registration.data_root,'--collection',registration.collection,'--service-project',registration.binding,'mcp'],disabled:true},other:{command:'other-tool',args:['unchanged']}}}};
}
test('upgrade migrates exact owned configuration before retiring launchers and publishes completion only after verification',async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const {registration,command,configuration}=legacyConfiguration(f),filename=path.join(registration.root,'.mcp.json'),before=JSON.stringify(configuration);
        await fs.writeFile(filename,before);migrationEditor(f);
        context.mock.method(RuntimeHost.prototype,'inspectLegacyLaunchers',async()=>({schema:1,active:f.binding.runtimeSha256,launcher:{path:command,sha256:'ab'.repeat(32)},universal_launcher:null}));
        let removed=0;context.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async()=>{
            removed++;const current=JSON.parse(await fs.readFile(filename,'utf8'));
            assert.equal(current.mcpServers.airk.url,f.httpUrl);assert.equal(current.mcpServers.airk.command,undefined);assert.equal(current.mcpServers.airk.disabled,true);
            assert.deepEqual(current.mcpServers.other,configuration.mcpServers.other);assert.equal(current.keep,configuration.keep);
            assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false,'No premature successful-upgrade marker');
            return {schema:1,active:f.binding.runtimeSha256,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[{fixture:'held native image audit'}]}};
        });
        await f.commands.get('airk.installCli')!();assert.equal(removed,1);
        assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')));
        const imported=f.state.httpConnections.find((value:any)=>value.client==='claude');assert.ok(imported);assert.equal(imported.selection.kind,'imported_legacy');
        assert.equal(imported.selection.selection.configuration_receipt,createHash('sha256').update(before).digest('hex'));
        assert.deepEqual(imported.selection.selection.registration,{binding:registration.binding,root:registration.root,data_root:registration.data_root,collection:registration.collection});
        assert.equal(f.calls.some(value=>value.includes('upgrade completion blocked')),false);
    }finally{await f.close();}
});
for(const appearedDuringCleanup of [false,true])test(`one install adopts a standalone sibling and migrates managed stdio without collapsing scope (${appearedDuringCleanup?'fresh original declaration appears during cache retirement':'same original file'})`,async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const {registration,configuration}=legacyConfiguration(f),destination=path.join(registration.root,'.mcp.json'),alias='old_standalone',digest='ba'.repeat(32),origin='10'.repeat(32),cached=path.join(f.root,'private-editor','User','globalStorage','iokaio.airk','helpers','1.3.1-windows-x64-'+digest.slice(0,16),'airk.exe');
        const fields={command:cached,args:['--root',registration.root,'--data-dir',path.join(f.root,'old-standalone-data'),'--collection','old-saved','mcp'],disabled:true,alwaysAllow:['airk_search']};
        const initial={...configuration,mcpServers:{...configuration.mcpServers,[alias]:fields}},before=JSON.stringify(initial),sha=createHash('sha256').update(before).digest('hex');if(!appearedDuringCleanup)await fs.writeFile(destination,before);migrationEditor(f);
        const original={binding:'f0'.repeat(32),root:registration.root,data_root:path.join(f.root,'old-standalone-data'),collection:'old-saved'},adopted={...original,binding:'f1'.repeat(32),data_root:path.join(f.root,'converted-saved-data')},namespace='34'.repeat(16);
        const claim={schema:1,origin_sha256:origin,config_sha256:sha,executable_sha256:digest,selected_entry_sha256:digest,container_path:['mcpServers'],original_server:alias,no_default_project:false};
        const setup={caller:'11'.repeat(16),key:{namespace,epoch:1,nonce:'22'.repeat(16)},expected_catalog_revision:1,name:'Original saved index',registration:adopted,sources:[{canonical_path:registration.root,aliases:[],owner:true}],setup_watch:false};
        const plan={schema:1,setup,original_registration:original,origin_sha256:origin,config_sha256:sha,executable_sha256:digest,quiescence_sha256:digest,archive_inventory_sha256:digest,original_publication_sha256:digest,source_inventory_sha256:digest};
        const adoption={schema:1,operation_id:'33'.repeat(16),plan,publication:{schema:1,plan_digest:digest,archive_inventory_sha256:digest,original_publication_sha256:digest,current:`airk-r1:${namespace}:2:${digest}`,resource_sha256:digest,compatibility_sha256:digest},original_server:alias,no_default_project:false,configuration_claims:[claim]};
        const grant={grant:{id:'grt_'+'92'.repeat(16),host:digest,revision:1,revoked:false,memberships:[{membership:{project_id:'prj_'+'44'.repeat(16),index_id:'idx_'+'55'.repeat(16),revision:1,scope:[{source_root_id:'src_'+'66'.repeat(16),kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null}]},provenance:{kind:'imported_standalone_adoption',adoption}};
        let accepted:any,adoptions=0,replans=0,retirements=0,originalPlan:any;
        const oldLocator={path:path.join(registration.root,'previous-original.json'),format:'mcp_servers_json',server:'previous_original',container_path:['mcpServers']},oldProof={config_path:oldLocator.path,server:oldLocator.server,container_path:['mcpServers'],config_sha256:'81'.repeat(32),executable:path.join(f.root,'previous-cache','airk.exe'),executable_sha256:'82'.repeat(32),selected_entry_sha256:'83'.repeat(32),declaration:{argv:['mcp']}};
        const newPlan=(selected:typeof f.binding,storage:string,locators:any[])=>({schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:storage,global_storage_roots:[storage],caches:[{executable:cached}],inspection_sha256:'ad'.repeat(32),origin_locators:locators,adoption_locators:locators,managed_locators:[],origin_classifications:[{locator:locators[0],origin_sha256:origin,kind:'standalone',authority:'not_observed'}],origin_proofs:[{config_path:destination,server:alias,container_path:['mcpServers'],config_sha256:sha,executable:cached,executable_sha256:digest,selected_entry_sha256:digest,declaration:{argv:fields.args}}]});
        context.mock.method(RuntimeHost.prototype,'prepareLegacyCaches',async(selected:typeof f.binding,storage:string,input?:string)=>{
            const locators=input?JSON.parse(await fs.readFile(input,'utf8')):[];
            if(accepted)return accepted;
            if(appearedDuringCleanup){assert.deepEqual(locators,[]);accepted={schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:storage,global_storage_roots:[storage],caches:[{executable:oldProof.executable}],inspection_sha256:'ac'.repeat(32),origin_locators:[oldLocator],adoption_locators:[],managed_locators:[oldLocator],origin_classifications:[{locator:oldLocator,origin_sha256:'84'.repeat(32),kind:'managed',authority:'not_observed'}],origin_proofs:[oldProof]};originalPlan=structuredClone(accepted);return accepted;}
            assert.equal(locators.length,1);assert.equal(await fs.readFile(destination,'utf8'),before);accepted=newPlan(selected,storage,locators);return accepted;
        });
        if(appearedDuringCleanup){
            context.mock.method(RuntimeHost.prototype,'retireLegacyCaches',async(_selected:typeof f.binding,prepared:any)=>{assert.equal(prepared,accepted);if(++retirements===1){await fs.writeFile(destination,before);throw new ServiceError('legacy_cache_retirement_unconfirmed',true,{code:'new_native_cache_observed'});}assert.equal(adoptions,1);assert.equal(prepared.origin_proofs[0],oldProof);return {scope:'context-only',caches_retired:true,remaining:0};});
            context.mock.method(RuntimeHost.prototype,'reprepareLegacyCaches',async(selected:typeof f.binding,previous:any,input?:string)=>{replans++;assert.deepEqual(previous,originalPlan);assert.ok(input,'A fresh protected native locator input must accompany explicit reprepare');const locators=JSON.parse(await fs.readFile(input,'utf8'));assert.deepEqual(locators,[{path:destination,format:'mcp_servers_json',server:alias,container_path:['mcpServers']}]);assert.equal(await fs.readFile(destination,'utf8'),before);const fresh=newPlan(selected,String(previous.context_global_storage),locators);accepted={...fresh,caches:[...previous.caches,...fresh.caches],origin_locators:[...previous.origin_locators,...fresh.origin_locators],managed_locators:previous.managed_locators,origin_classifications:[...previous.origin_classifications,...fresh.origin_classifications],origin_proofs:[...previous.origin_proofs,...fresh.origin_proofs]};return accepted;});
        }
        context.mock.method(RuntimeHost.prototype,'adoptLegacyIndexes',async(_selected:typeof f.binding,prepared:any,_operation:string,input:string)=>{adoptions++;assert.equal(prepared,accepted);assert.equal(await fs.readFile(destination,'utf8'),before);assert.deepEqual(JSON.parse(await fs.readFile(input,'utf8')),accepted.adoption_locators);return {schema:1,status:'ready',claims:[{locator:accepted.adoption_locators[0],origin_sha256:origin,config_sha256:sha,group_operation_id:setup.key.nonce,original_registration:original,registration:adopted,claim}],groups:[{grant}],future_native_metadata:{retained:true}};});
        const fixtureLegacy=RuntimeHost.prototype.httpLegacyConnection,fixtureHttp=RuntimeHost.prototype.httpConnection;
        context.mock.method(RuntimeHost.prototype,'httpLegacyConnection',async function(this:RuntimeHost,selected:typeof f.binding,saved:any,file:string,receipt:string,id:string,client:string,descriptor?:string,originSha?:string){
            if(saved.binding!==original.binding)return fixtureLegacy.call(this,selected,saved,file,receipt,id,client,descriptor,originSha);
            assert.equal(originSha,origin);assert.equal(receipt,sha);assert.deepEqual(saved,original);
            return fixtureHttp.call(this,selected,{kind:'imported_legacy',selection:{namespace,expected_catalog_revision:2,registration:adopted,configuration_receipt:sha,grant}},file,id,client,descriptor);
        });
        await f.commands.get('airk.installCli')!();
        const after=JSON.parse(await fs.readFile(destination,'utf8')),managedKey='airk-'+registration.collection;
        assert.equal(adoptions,1);assert.equal(after.mcpServers[alias].url,f.httpUrl);assert.equal(after.mcpServers[managedKey].url,f.httpUrl);assert.equal(after.mcpServers[alias].disabled,true);assert.deepEqual(after.mcpServers[alias].alwaysAllow,['search']);
        assert.deepEqual(after.mcpServers.other,initial.mcpServers.other);assert.equal(after.mcpServers[alias].command,undefined);assert.equal(after.mcpServers[managedKey].command,undefined);
        const clients=f.state.httpConnections.filter(value=>value.client==='claude');assert.equal(clients.length,2);assert.notEqual(clients[0].id,clients[1].id);assert.equal(clients[0].selection.kind,'imported_legacy');assert.equal(clients[1].selection.kind,'imported_legacy');
        for(const client of clients)assert.equal((client.selection.selection as any).configuration_receipt,sha,'Both original claims retain the original complete file SHA before either CAS');
        assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')));assert.equal(f.calls.some(value=>value.includes('upgrade completion blocked')),false,'A managed sibling does not prematurely terminate standalone snapshot tracking');
        assert.equal(replans,appearedDuringCleanup?1:0);if(appearedDuringCleanup){assert.equal(retirements,2);assert.deepEqual(accepted.origin_proofs[0],originalPlan.origin_proofs[0]);assert.deepEqual(accepted.origin_locators[0],originalPlan.origin_locators[0]);}
    }finally{await f.close();}
});

const standaloneResumeCases:{reattach:boolean;partial:boolean;headless:boolean;historical:boolean;disable:boolean;refusal?:string}[]=[
    {reattach:false,partial:true,headless:true,historical:false,disable:false},
    {reattach:true,partial:false,headless:false,historical:false,disable:false},
    {reattach:true,partial:true,headless:false,historical:false,disable:false},
    {reattach:true,partial:true,headless:true,historical:false,disable:false},
    {reattach:true,partial:true,headless:false,historical:true,disable:false},
    {reattach:true,partial:true,headless:false,historical:true,disable:true},
    ...['owned_connection_revoked','owned_connection_unavailable','owned_connection_expired'].map(refusal=>({reattach:true,partial:true,headless:false,historical:true,disable:false,refusal})),
];
for(const {reattach,partial,headless,historical,disable,refusal}of standaloneResumeCases)test(`two clients sharing one adopted index resume the frozen batch and per-file SHA (${reattach?'new':'same'} attachment, ${partial?'partial':'no'} file CAS, ${headless?'headless':historical?'historical-base-ID-attached':'attached'} original${disable?', disable Assistance':''}${refusal?', '+refusal:''})`,async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,assistance:!headless});
    try{
        const root=f.folders[0].uri.fsPath,files=[path.join(root,'.codex','config.toml'),path.join(root,'.mcp.json')],keys=['old_codex','old_claude'],digest='bc'.repeat(32),cached=path.join(f.root,'User','globalStorage','iokaio.airk','helpers','1.3.1-windows-x64-'+digest.slice(0,16),'airk.exe'),args=['--root',root,'--data-dir',path.join(f.root,'original-data'),'--collection','saved','mcp'];
        const sources=[`# Original Codex policy\n[mcp_servers.old_codex]\ncommand = ${JSON.stringify(cached)}\nargs = ${JSON.stringify(args)}\nenabled = false\n`,JSON.stringify({keep:'untouched',mcpServers:{old_claude:{command:cached,args,disabled:true}}})];
        for(const [index,file]of files.entries()){await fs.mkdir(path.dirname(file),{recursive:true});await fs.writeFile(file,sources[index]);}const documents=migrationEditor(f);
        const locators:LegacyOriginLocator[]=files.map((file,index)=>({path:file,format:index?'mcp_servers_json':'codex_toml',server:keys[index],container_path:[index?'mcpServers':'mcp_servers']})),shas=sources.map(source=>createHash('sha256').update(source).digest('hex')),origins=['10'.repeat(32),'20'.repeat(32)],namespace='34'.repeat(16),original={binding:'f2'.repeat(32),root,data_root:path.join(f.root,'original-data'),collection:'saved'},adopted={...original,binding:'f3'.repeat(32),data_root:path.join(f.root,'converted-data')};
        const claims=locators.map((locator,index)=>({schema:1,origin_sha256:origins[index],config_sha256:shas[index],executable_sha256:digest,selected_entry_sha256:digest,container_path:locator.container_path,original_server:locator.server,no_default_project:false}));
        const setup={caller:'11'.repeat(16),key:{namespace,epoch:1,nonce:'22'.repeat(16)},expected_catalog_revision:1,name:'Shared saved index',registration:adopted,sources:[{canonical_path:root,aliases:[],owner:true}],setup_watch:false},plan={schema:1,setup,original_registration:original,origin_sha256:origins[0],config_sha256:shas[0],executable_sha256:digest,quiescence_sha256:digest,archive_inventory_sha256:digest,original_publication_sha256:digest,source_inventory_sha256:digest};
        const adoption={schema:1,operation_id:'33'.repeat(16),plan,publication:{schema:1,plan_digest:digest,archive_inventory_sha256:digest,original_publication_sha256:digest,current:`airk-r1:${namespace}:2:${digest}`,resource_sha256:digest,compatibility_sha256:digest},original_server:keys[0],no_default_project:false,configuration_claims:claims},grant={grant:{id:'grt_'+'93'.repeat(16),host:digest,revision:1,revoked:false,memberships:[{membership:{project_id:'prj_'+'44'.repeat(16),index_id:'idx_'+'55'.repeat(16),revision:1,scope:[{source_root_id:'src_'+'66'.repeat(16),kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null}]},provenance:{kind:'imported_standalone_adoption',adoption}};
        const prepared:any={schema:1,active:f.binding.runtimeSha256,scope:'context-only',context_global_storage:f.root,global_storage_roots:[f.root],caches:[{executable:cached}],inspection_sha256:'ae'.repeat(32),origin_locators:locators,adoption_locators:locators,managed_locators:[],origin_classifications:locators.map((locator,index)=>({locator,origin_sha256:origins[index],kind:'standalone',authority:'not_observed'})),origin_proofs:locators.map((locator,index)=>({config_path:locator.path,server:locator.server,container_path:locator.container_path,config_sha256:shas[index],executable:cached,executable_sha256:digest,selected_entry_sha256:digest,declaration:{argv:args}}))};
        context.mock.method(RuntimeHost.prototype,'prepareLegacyCaches',async()=>prepared);
        const owner=f.context.subscriptions.find((value:any)=>typeof value.adoptStandaloneConfigurations==='function');assert.ok(owner);
        const seed=async()=>{owner.legacyCachePreparation=prepared;owner.legacyMigrationFiles=new Map(await Promise.all(files.map(async(file,index)=>{const source=await fs.readFile(file,'utf8'),agent=index?'claude':'codex';return [file,{agent,file:{agent,destination:file,source,sha256:createHash('sha256').update(source).digest('hex'),declarations:globalInventory.globalConfigDeclarations(agent,source)}}] as const;})));};await seed();
        const requests:{operation:string;bytes:string}[]=[];let conversions=0;
        context.mock.method(RuntimeHost.prototype,'adoptLegacyIndexes',async(_binding:typeof f.binding,accepted:any,operation:string,input:string)=>{assert.equal(accepted,prepared);requests.push({operation,bytes:await fs.readFile(input,'utf8')});if(requests.length===1)conversions++;return {retained_completed_operation:requests.length>1,groups:[{grant}],claims:locators.map((locator,index)=>({locator,origin_sha256:origins[index],config_sha256:shas[index],group_operation_id:setup.key.nonce,original_registration:original,registration:adopted,claim:claims[index]}))};});
        const fixtureHttp=RuntimeHost.prototype.httpConnection;
        const reattachments:string[]=[];
        context.mock.method(RuntimeHost.prototype,'httpLegacyConnection',async function(this:RuntimeHost,binding:typeof f.binding,saved:any,file:string,receipt:string,id:string,client:string,descriptor?:string,origin?:string,reattachId?:string){const index=origins.indexOf(origin!);assert.ok(index>=0);assert.equal(receipt,shas[index]);assert.deepEqual(saved,original);if(reattachId){reattachments.push(reattachId);assert.notEqual(id,reattachId,'A retired native credential ID is never reused');}return fixtureHttp.call(this,binding,{kind:'imported_legacy',selection:{namespace,expected_catalog_revision:2,registration:adopted,configuration_receipt:receipt,grant}},file,id,client,descriptor);});
        const open=f.stub.workspace.openTextDocument;let fail=true;
        f.stub.workspace.openTextDocument=async(uri:any)=>{if(uri.fsPath===files[partial?1:0]&&fail){fail=false;throw Error('fixture editor stopped before file CAS');}return open(uri);};
        await assert.rejects(owner.adoptStandaloneConfigurations(f.binding,()=>{},()=>{}),/editor stopped/);
        if(partial)assert.ok((await fs.readFile(files[0],'utf8')).includes(f.httpUrl));else assert.equal(await fs.readFile(files[0],'utf8'),sources[0]);
        assert.equal(await fs.readFile(files[1],'utf8'),sources[1]);
        if(historical){
            // The actual HEAD predecessor used the base ID even when it
            // attached to an Explorer window. Native provenance, not ID shape,
            // must distinguish that credential from an active manual one.
            const originalConnections=[...f.state.httpConnections];
            for(const [index,locator]of locators.entries()){
                const parts=['standalone-http/1',f.binding.installDir,legacyOriginLocatorIdentity(locator),origins[index],index?'claude':'codex'],id=createHash('sha256').update(JSON.stringify(parts)).digest('hex').slice(0,32),oldRecord=originalConnections[index];
                const old=[...f.values.values()].find((value:any)=>value?.connection_id===oldRecord.id) as any;assert.ok(old);
                const info=await fixtureHttp.call(owner.runtime(),f.binding,oldRecord.selection,'fixture-native-input',id,oldRecord.client,oldRecord.descriptor);
                for(const [key,value]of f.values)if(key.startsWith('airk.http.standalone.')&&(value as any)?.connection_id===oldRecord.id)f.values.set(key,structuredClone(info));
                if(partial&&index===0){const document=documents.get(files[index]);document.replace(refreshHttpAgentConfig('codex',document.getText(),info,old.endpoint.url,keys[index],old));await document.save();}
            }
            f.state.httpConnections.splice(0,originalConnections.length);
        }
        const issued=f.state.httpConnections.length;
        const historicalMetadata=new Map<string,unknown>(),retired=new Set(f.state.httpConnections.map(value=>value.id));
        // Model the predecessor's attachment-independent records left by an
        // interrupted installation. Exact headless identities stay reusable;
        // a replacement window preserves the old metadata and enrolls its own.
        for(const [index,locator]of locators.entries()){
            const parts=['standalone-http/1',f.binding.installDir,legacyOriginLocatorIdentity(locator),origins[index],index?'claude':'codex'];
            const key='airk.http.standalone.1.'+createHash('sha256').update(JSON.stringify(parts)).digest('hex');
            const previous=[...f.values.values()].find((value:any)=>value?.connection_id===f.state.httpConnections[index].id);
            assert.ok(previous);f.values.set(key,structuredClone(previous));historicalMetadata.set(key,structuredClone(previous));
            for(const [savedKey,value]of f.values)if(savedKey.startsWith('airk.http.standalone.2.')&&(value as any)?.connection_id===f.state.httpConnections[index].id)f.values.delete(savedKey);
            f.values.delete('airk.http.standalone.origin.1.'+createHash('sha256').update(JSON.stringify(parts)).digest('hex'));
        }
        if(reattach){
            const lookup=RuntimeHost.prototype.httpConnectionInfo;
            context.mock.method(RuntimeHost.prototype,'httpConnectionInfo',async function(this:RuntimeHost,binding:typeof f.binding,id:string){if(!headless&&retired.has(id))throw new ServiceError('owned_connection_revoked');return lookup.call(this,binding,id);});
            const ownership=RuntimeHost.prototype.lookupHttpConnection;
            context.mock.method(RuntimeHost.prototype,'lookupHttpConnection',async function(this:RuntimeHost,binding:typeof f.binding,id:string,expected?:Parameters<RuntimeHost['lookupHttpConnection']>[2]){
                if(!headless&&retired.has(id)){
                    if(refusal)throw new ServiceError(refusal);
                    if(!expected)throw new ServiceError('owned_connection_attachment_retired');
                    const old=expected.connection;assert.equal(old.connection_id,id);
                    return {schema:1,active:binding.runtimeSha256,connection_id:id,status:'unavailable',code:'owned_connection_attachment_retired',attachment_retirement:{schema:1,connection_id:id,installation:old.endpoint.installation,namespace,client:expected.client==='claude'?'claude_code':expected.client,expires_unix_ms:Date.now()+60_000,explorer_descriptor_sha256:'ab'.repeat(32),authority_sha256:httpAuthorityDigest(old.selection)}};
                }
                return ownership.call(this,binding,id,expected);
            });
            f.settings.set('explorer.assistance.enabled',!disable);
            await owner.startAssistance();
            assert.notEqual(owner.httpAttachment(),f.state.httpConnections[0].descriptor);
        }
        if(disable||refusal){
            const before=await Promise.all(files.map(file=>fs.readFile(file,'utf8')));
            await assert.rejects(owner.migrateAgentConnections(f.binding),(error:unknown)=>error instanceof ServiceError&&error.code===(refusal??'http_attachment_reconnect_requires_live_window'));
            assert.deepEqual(await Promise.all(files.map(file=>fs.readFile(file,'utf8'))),before,'Refused recovery preserves each exact config preimage');
            assert.equal(f.state.httpConnections.length,issued);assert.deepEqual(reattachments,[],'No uncertain, explicitly revoked, expired or headless-fallback credential is reissued');
            for(const [key,value]of historicalMetadata)assert.deepEqual(f.values.get(key),value);
            return;
        }
        await owner.migrateAgentConnections(f.binding);
        assert.equal(conversions,1,'Native completed publication is retained, not copied once per client or retry');assert.equal(requests.length,2);assert.deepEqual(requests[1],requests[0],'Original complete batch request survives a prior file already becoming HTTPS');
        const added=reattach&&!headless?2:0;assert.equal(f.state.httpConnections.length,issued+added,'Only proven retired attachments enroll new credentials; existing live headless/manual credentials remain');
        assert.equal(reattachments.length,added,'Every replacement passes the exact old ID for native mutation-side retirement recheck');
        for(const [key,value]of historicalMetadata)assert.deepEqual(f.values.get(key),value,'Historical credential metadata remains untouched');
        const after=JSON.parse(await fs.readFile(files[1],'utf8'));assert.equal(after.keep,'untouched');assert.equal(after.mcpServers.old_claude.disabled,true);assert.equal(after.mcpServers.old_claude.url,f.httpUrl);
        const finalIds=await Promise.all(files.map(async(file,index)=>{const agent=index?'claude':'codex',row=globalInventory.globalConfigDeclarations(agent,await fs.readFile(file,'utf8')).find(row=>row.serverKey===keys[index])!;return httpConfigurationConnectionId(agent,row.fields);}));
        const selected=finalIds.map(id=>f.state.httpConnections.find(value=>value.id===id)!.selection as any);assert.deepEqual(selected.map(value=>value.selection.registration),[adopted,adopted]);assert.deepEqual(selected.map(value=>value.selection.configuration_receipt),shas);assert.deepEqual(selected.map(value=>value.selection.grant),[grant,grant]);
        if(reattach){
            const resumed=added?f.state.httpConnections.slice(-added):[];assert.ok(resumed.every(value=>!retired.has(value.id)&&value.descriptor===owner.httpAttachment()));
            if(headless)assert.equal(finalIds[0],f.state.httpConnections[0].id,'Enabling Assistance does not replace the already saved headless/manual credential');
            await seed();await owner.adoptStandaloneConfigurations(f.binding,()=>{},()=>{});
            assert.equal(f.state.httpConnections.length,issued+added,'A retry under the current attachment reuses its own credentials');
        }
    }finally{await f.close();}
});

test('cache native preimages precede original config edits and cache uncertainty prevents launcher/runtime completion',async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const {registration,configuration}=legacyConfiguration(f),destination=path.join(registration.root,'.mcp.json'),before=JSON.stringify(configuration);await fs.writeFile(destination,before);migrationEditor(f);
        let accepted:any,attempts=0;
        context.mock.method(RuntimeHost.prototype,'prepareLegacyCaches',async(selected:typeof f.binding,storage:string)=>{
            assert.equal(selected.runtimeSha256,f.binding.runtimeSha256);
            if(!accepted){assert.equal(await fs.readFile(destination,'utf8'),before,'Original complete bytes exist when native accepts its cleanup plan');accepted={schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:storage,global_storage_roots:[storage],caches:[],origin_proofs:[],origin_locators:[],adoption_locators:[],managed_locators:[],origin_classifications:[],inspection_sha256:'ab'.repeat(32)};}
            return accepted;
        });
        context.mock.method(RuntimeHost.prototype,'retireLegacyCaches',async(selected:typeof f.binding,prepared:Record<string,unknown>)=>{
            assert.equal(prepared,accepted);assert.equal(selected.runtimeSha256,prepared.active);
            const current=JSON.parse(await fs.readFile(destination,'utf8'));assert.equal(current.mcpServers.airk.url,f.httpUrl,'Migration is saved before native can retire old executable images');
            if(++attempts===1)throw new ServiceError('legacy_cache_process_identity_unknown',true,{scope:'context-only',processes:{remaining:null,observations:[{unobservable:true}]}});
            return {scope:'context-only',global_storage_roots:prepared.global_storage_roots,caches_retired:true,remaining:0};
        });
        await f.commands.get('airk.installCli')!();
        assert.equal(f.calls.includes('legacy-launchers-retired'),false);assert.equal(f.calls.includes('previous-runtime-retired'),false);assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
        const migrated=f.state.httpConnections.filter(value=>value.client==='claude');assert.equal(migrated.length,1);await f.commands.get('airk.installCli')!();
        assert.equal(attempts,2);assert.deepEqual(f.state.httpConnections.filter(value=>value.client==='claude'),migrated,'Resumption verifies already migrated HTTP authority instead of enrolling it again');
        assert.ok(f.calls.includes('legacy-launchers-retired'));assert.ok(f.calls.includes('previous-runtime-retired'));assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')));
    }finally{await f.close();}
});

for(const obstruction of ['dirty','save_failed','unsupported_client','native_unknown'] as const){
    test(`upgrade stays incomplete and never retires launchers before ${obstruction} is resolved`,async context=>{
        const f=await serviceTestHost(context,{automatic:false});
        try{
            const {registration,configuration}=legacyConfiguration(f);
            const filename=obstruction==='unsupported_client'?path.join(registration.root,'.gemini','settings.json'):path.join(registration.root,'.mcp.json');
            await fs.mkdir(path.dirname(filename),{recursive:true});const before=JSON.stringify(configuration);await fs.writeFile(filename,before);
            migrationEditor(f,obstruction==='save_failed');
            if(obstruction==='dirty'){const document=await f.stub.workspace.openTextDocument(f.stub.Uri.file(filename));document.isDirty=true;}
            let removed=0;context.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async()=>{removed++;throw new ServiceError('legacy_launcher_process_identity_unknown',true,{observations:[{unknown:true}]});});
            await f.commands.get('airk.installCli')!();
            assert.equal(removed,obstruction==='native_unknown'?1:0);
            assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
            assert.ok(f.calls.some(value=>value.includes('upgrade completion blocked')));
            if(obstruction!=='native_unknown')assert.equal(await fs.readFile(filename,'utf8'),before,'Unconfirmed source file preserved');
        }finally{await f.close();}
    });
}

for(const flag of ['--data-dir','--service-project'] as const)test(`generated legacy migration refuses a crossed original ${flag} instead of selecting by source root`,async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const {registration,configuration}=legacyConfiguration(f),destination=path.join(registration.root,'.mcp.json'),entry=configuration.mcpServers['airk-'+registration.collection],index=entry.args.indexOf(flag);entry.args[index+1]=flag==='--data-dir'?path.join(f.root,'different-index-data'):'fe'.repeat(32);
        const before=JSON.stringify(configuration);await fs.writeFile(destination,before);migrationEditor(f);await f.commands.get('airk.installCli')!();
        assert.equal(await fs.readFile(destination,'utf8'),before);assert.equal(f.state.httpConnections.filter(value=>value.client==='claude').length,0);assert.equal(f.calls.includes('legacy-launchers-retired'),false);assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
    }finally{await f.close();}
});
for(const secondFailure of [false,true])test(`cache inventory recovery performs one native reprepare and complete migration review (${secondFailure?'second refusal remains incomplete':'new inventory completes'})`,async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false});
    try{
        let current:any,retirements=0,replans=0,preparations=0;
        context.mock.method(RuntimeHost.prototype,'prepareLegacyCaches',async(selected:typeof f.binding,storage:string)=>{preparations++;current??={schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:storage,global_storage_roots:[storage],caches:[],origin_proofs:[],origin_locators:[],adoption_locators:[],managed_locators:[],origin_classifications:[],inspection_sha256:'a1'.repeat(32)};return current;});
        context.mock.method(RuntimeHost.prototype,'retireLegacyCaches',async(_binding:typeof f.binding,prepared:Record<string,unknown>)=>{retirements++;assert.equal(prepared,current);if(retirements===1||secondFailure)throw new ServiceError('legacy_cache_retirement_unconfirmed',true,{original_native_failure:{remaining:null}});return {scope:'context-only',caches_retired:true,remaining:0};});
        context.mock.method(RuntimeHost.prototype,'reprepareLegacyCaches',async(_binding:typeof f.binding,previous:Record<string,unknown>)=>{replans++;assert.equal(previous,current);current={...current,inspection_sha256:'a2'.repeat(32)};return current;});
        await f.commands.get('airk.installCli')!();assert.equal(replans,1);assert.equal(retirements,2);assert.equal(preparations,2,'Replacement inventory triggers the full guarded migration phase');assert.equal(f.state.httpConnections.length,0);
        assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),!secondFailure);assert.equal(f.calls.includes('legacy-launchers-retired'),!secondFailure);
    }finally{await f.close();}
});

test('inactive native roots with unrelated agent settings neither enroll nor block launcher retirement',async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const registration=[...f.registrations.values()][0],filename=path.join(registration.root,'.mcp.json'),bytes='{"account":"keep","mcpServers":{"unrelated":{"command":"other"}}}';
        await fs.writeFile(filename,bytes);f.registrations.delete(registration.binding);f.inactiveRegistrations.set(registration.binding,registration);
        await f.commands.get('airk.installCli')!();
        assert.equal(await fs.readFile(filename,'utf8'),bytes);assert.equal(f.state.httpConnections.length,0);
        assert.ok(f.calls.includes('legacy-launchers-retired'));assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')));
    }finally{await f.close();}
});
for(const shape of ['nested_owned','owned_custom_alias','owned_unknown_scope','foreign'] as const){
    test(`global ${shape} declaration is migrated or blocks cleanup using original native authority`,async context=>{
        const realInventory=realGlobalInventory,f=await serviceTestHost(context,{automatic:false});
        try{
            const {registration,configuration}=legacyConfiguration(f),home=path.join(f.root,'isolated-global-home');await fs.mkdir(home);
            const entry=configuration.mcpServers['airk-'+registration.collection];
            if(shape==='owned_unknown_scope')entry.args=entry.args.map(arg=>arg===registration.root?path.join(f.root,'unregistered-source'):arg);
            if(shape==='foreign'){entry.command=path.join(f.root,'foreign-install','airk-launcher');entry.args=entry.args.map(arg=>arg===f.binding.installDir?path.dirname(entry.command):arg);}
            const key=shape==='owned_custom_alias'?'custom_named_airk':'airk-'+registration.collection;
            const value={account:'preserve',projects:{'this key is never source authority':{mcpServers:{[key]:entry,unrelated:{command:'other'}}},foreign:{mcpServers:{other:{command:'foreign'}}}}};
            const filename=path.join(home,'.claude.json'),before=JSON.stringify(value);await fs.writeFile(filename,before);migrationEditor(f);
            context.mock.method(globalInventory,'inventoryGlobalAgentConfigs',async(options:globalInventory.GlobalInventoryOptions)=>realInventory({...options,home,env:{},globalStoragePath:undefined,vscodeUserDirectories:[]}));
            let cleanup=0;context.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async()=>{cleanup++;return {schema:1,active:f.binding.runtimeSha256,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[]}};});
            await f.commands.get('airk.installCli')!();
            if(shape==='nested_owned'){
                assert.equal(cleanup,1);const after=JSON.parse(await fs.readFile(filename,'utf8'));
                const servers=after.projects['this key is never source authority'].mcpServers;assert.equal(servers.airk.url,f.httpUrl);assert.equal(servers.airk.disabled,true);assert.deepEqual(servers.unrelated,value.projects['this key is never source authority'].mcpServers.unrelated);
                assert.deepEqual(after.projects.foreign,value.projects.foreign);assert.equal(after.account,value.account);
                assert.equal((f.state.httpConnections[0].selection.selection as any).configuration_receipt,createHash('sha256').update(before).digest('hex'),'Actual complete original global bytes bind enrollment');
            }else{
                assert.equal(await fs.readFile(filename,'utf8'),before);assert.equal(cleanup,shape==='foreign'?1:0);assert.equal(f.state.httpConnections.filter((row:any)=>row.client==='claude').length,0);
            }
        }finally{await f.close();}
    });
}
test('interrupted native cleanup resumes without stale completion and repeated runtime replacement reruns finalization',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false});
    try{
        let attempts=0;context.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async()=>{attempts++;if(attempts===1)throw new ServiceError('legacy_launcher_identity_unknown',true,{phase:'removing'});return {schema:1,active:f.binding.runtimeSha256,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[]}};});
        await f.commands.get('airk.installCli')!();assert.equal(attempts,1);assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
        await f.commands.get('airk.installCli')!();assert.equal(attempts,2);const key='airk.installComplete.2.'+f.binding.installDir,first=f.values.get(key);assert.ok(first);
        f.binding.runtimeSha256='ef'.repeat(32);await f.commands.get('airk.installCli')!();assert.equal(attempts,3);assert.notEqual(f.values.get(key),first);assert.equal(f.values.get(key),f.binding.version+':'+f.binding.runtimeSha256);
        assert.ok(f.calls.filter(value=>value==='previous-runtime-retired').length>=2,'Each completed distinct runtime retires previous payloads after the gate');
    }finally{await f.close();}
});

test('future modern upgrade completes an older fully occupied cleanup before admitting another payload',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.15',runtimeVersion:'1.8.14',installedVersion:'1.8.15'});
    try{
        f.binding.launcher=null;const order:string[]=[];
        context.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async(selected:typeof f.binding)=>{order.push('launchers-'+selected.version);return {schema:1,active:selected.runtimeSha256,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[]}};});
        context.mock.method(RuntimeHost.prototype,'retirePrevious',async(selected:typeof f.binding)=>{order.push('runtimes-'+selected.version);return {active:selected.runtimeSha256,retired_previous:null,retired_runtimes:[],complete:true,selected_service_restarted:false,processes:{scope:'verified_superseded_runtime_images',remaining:0,observations:[]}};});
        const transition=RuntimeHost.prototype.transition;
        context.mock.method(RuntimeHost.prototype,'transition',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['transition']>){order.push('activate-next');return transition.apply(this,args);});
        await f.commands.get('airk.installCli')!();
        assert.deepEqual(order,['launchers-1.8.14','runtimes-1.8.14','activate-next','launchers-1.8.15','runtimes-1.8.15']);
        assert.ok(f.calls.includes('selected-modern-metadata-verified'),'Older selected binding uses its narrowly verified metadata adapter');
        assert.equal(f.values.get('airk.installComplete.2.'+f.binding.installDir),'1.8.15:'+f.binding.runtimeSha256);
        assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project||value.enqueue).length,0);
    }finally{await f.close();}
});

for(const [transientFailures,directoryHint] of [[0,false],[2,false],[0,true]] as const)test(`live native owner replacement refreshes owned HTTP port after ${transientFailures} transient failures (${directoryHint?'closed UI directory watcher':'console observation'}) without installing or enrolling again`,async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const definitions=await f.api.getServerDefinitions();assert.equal(definitions.length,1);
        const id=String(definitions[0].headers.Authorization).split('.')[1],info=await RuntimeHost.prototype.httpConnectionInfo.call({} as RuntimeHost,f.binding,id),selection=structuredClone(info.selection);
        const row=[...f.registrations.values()][0],destination=path.join(row.root,'.vscode','mcp.json');await fs.mkdir(path.dirname(destination));
        await fs.writeFile(destination,JSON.stringify({servers:{airk:{type:'http',url:info.endpoint.url,headers:{Authorization:'Bearer ${input:airkToken_'+id+'}','X-AIRK-Response-Mode':'full'},disabled:true},other:{command:'keep'}}}));migrationEditor(f);
        const control=(f.views.get('airk.workbench') as any).host.control;
        if(directoryHint){
            assert.equal((control as any).consumers,0,'No Workbench or Explorer consumer polls the service');
            const owner=f.context.subscriptions.find((entry:any)=>typeof entry.syncHttpEndpointWatcher==='function');assert.ok(owner);
            await fs.writeFile(path.join(f.binding.controlDir,'http-1','endpoint.json'),'old notification hint');owner.syncHttpEndpointWatcher();
            const limit=Date.now()+3000;while(owner.observedHttpInstance!=='12'.repeat(16)&&Date.now()<limit)await new Promise(resolve=>setTimeout(resolve,10));
            assert.equal(owner.observedHttpInstance,'12'.repeat(16),'Initial authenticated discovery closes the missed-event gap');
        }
        const next={...info,endpoint:{...info.endpoint,url:'https://127.0.0.1:57423/mcp',service_instance:'98'.repeat(16),generation:info.endpoint.generation+1}};
        let lookups=0;context.mock.method(RuntimeHost.prototype,'lookupHttpConnection',async(binding:typeof f.binding,id:string)=>{lookups++;if(lookups<=transientFailures)throw new ServiceError('native_management_busy',false,{safe_read:true});assert.equal(binding.runtimeSha256,f.binding.runtimeSha256);assert.equal(id,info.connection_id);return {schema:1,active:binding.runtimeSha256,connection_id:id,status:'owned',connection:next};});
        context.mock.method(RuntimeHost.prototype,'httpConnectionInfo',async()=>next);
        let changes=0;f.providers[0].onDidChangeMcpServerDefinitions(()=>changes++);
        const before=f.state.httpConnections.length,retirements=f.calls.filter(value=>value==='legacy-launchers-retired').length;
        if(!directoryHint)control.controller.state.health={state:'live',checkedAt:Date.now(),value:{status:'health',observation:{version:f.binding.version,runtime_sha256:f.binding.runtimeSha256,instance:next.endpoint.service_instance}}};
        const realNow=Date.now.bind(Date);let offset=0;
        if(transientFailures)context.mock.method(Date,'now',()=>realNow()+offset);
        const emit=()=>{(control as any).stateChanged.fire(control.controller.state);};
        const waitFor=async(predicate:()=>boolean)=>{const limit=realNow()+3000;while(!predicate()&&realNow()<limit)await new Promise(resolve=>setTimeout(resolve,10));assert.ok(predicate());};
        const noisyReads=f.nativeMethods.filter(method=>['resources','workers','events','embedding_status','source_status'].includes(method)).length;
        if(directoryHint){
            f.state.nativeInstance=next.endpoint.service_instance;f.state.nativeHealthFailures=1;
            const temporary=path.join(f.binding.controlDir,'http-1','endpoint.next');await fs.writeFile(temporary,'invalid descriptor bytes are only a notification hint');
            await fs.rename(temporary,path.join(f.binding.controlDir,'http-1','endpoint.json'));
        }else emit();
        for(let failure=1;failure<=transientFailures;failure++){
            await waitFor(()=>f.calls.filter(value=>value.includes('saved HTTPS connection refresh deferred')).length===failure);
            assert.equal(JSON.parse(await fs.readFile(destination,'utf8')).servers.airk.url,info.endpoint.url);
            emit();await new Promise(resolve=>setTimeout(resolve,15));assert.equal(lookups,failure,'Backoff prevents repeated state events from replaying even the safe read');
            // The next attempt must read the present file and preserve new edits.
            const current=JSON.parse(await fs.readFile(destination,'utf8'));current.recentEdit=failure;await fs.writeFile(destination,JSON.stringify(current));
            offset+=30000;emit();
        }
        const deadline=Date.now()+3000;let actual:any;
        do{actual=JSON.parse(await fs.readFile(destination,'utf8'));if(actual.servers.airk.url===next.endpoint.url&&changes>0)break;await new Promise(resolve=>setTimeout(resolve,10));}while(Date.now()<deadline);
        assert.equal(actual.servers.airk.url,next.endpoint.url);assert.equal(actual.servers.airk.disabled,true);assert.deepEqual(actual.servers.other,{command:'keep'});
        assert.ok(changes>0);assert.equal(lookups,1+transientFailures);if(transientFailures){assert.equal(actual.recentEdit,transientFailures);assert.equal(f.warnings.filter(value=>value.includes('Some saved HTTPS connections still need refreshing')).length,1);}assert.equal(f.state.httpConnections.length,before,'Port discovery never reenrolls a credential or changes its grant');
        assert.equal(f.calls.filter(value=>value==='legacy-launchers-retired').length,retirements,'Port refresh performs no installer retirement');assert.deepEqual(next.selection,selection);
        if(directoryHint){assert.equal(f.state.nativeHealthFailures,0,'A transient management failure after publication is retried without a second file event');assert.equal((control as any).consumers,0);assert.equal(f.nativeMethods.filter(method=>['resources','workers','events','embedding_status','source_status'].includes(method)).length,noisyReads,'Endpoint observation does not start general console polling');}
    }finally{await f.close();}
});
