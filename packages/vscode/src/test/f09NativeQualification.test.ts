// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {RuntimeHost} from '../serviceBootstrap';
import {record} from '../serviceClient';
import {assistanceEntry} from '../model';
import {probeAssistance,runText} from '../native';
import {writeLaunchDescriptor,assistanceDirectory,launchDescriptorPath} from '../explorerAssistanceLaunch';
import {NativeServiceFixture} from './nativeServiceFixture';
import {serviceTestHost} from './serviceTestHost';
const binary=process.env.AIRK_TEST_BINARY;
const qualify=process.env.AIRK_TEST_F09_QUALIFICATION==='1';
const extension=path.resolve(__dirname,'../..');

test('F09 real RuntimeHost source port publishes selected host scope without invoke mock',
    {skip:!binary||!qualify,timeout:360000},async context=>{
    // Positive early UI policy excludes managed pins; own the staged install.
    const fixture=await NativeServiceFixture.create(binary!,'owned');
    let editor:Awaited<ReturnType<typeof serviceTestHost>>|undefined;
    try{
        const spec=await fixture.register('source-0');
        const inspection=await fixture.run(['runtime','inspect','--install-dir',fixture.install]);
        assert.equal(inspection.managed_policy,null);
        const version=String(record(inspection.approval).version);
        const realHost=new RuntimeHost(extension,version,{channel:'stable',installDir:fixture.install});
        const binding=realHost.binding(inspection,fixture.install);
        const before=await fixture.run(['runtime','snapshot','--install-dir',fixture.install]);
        const direct=await realHost.sourceScope(binding,spec.root,spec.collection);
        assert.equal(direct.outcome,'registered');assert.equal(direct.registration!.binding,spec.service!.project);
        assert.equal(direct.namespace,record(before.lifecycle).namespace);
        // Negative/unknown metadata never becomes registration or a retry loop.
        const missingRoot=path.join(fixture.owner,'scope-never-created');
        const missing=await realHost.sourceScope(binding,missingRoot,null);
        assert.equal(missing.outcome,'missing');assert.equal(missing.registration,null);
        assert.equal(missing.namespace,direct.namespace);
        await assert.rejects(fs.stat(missingRoot),{code:'ENOENT'});
        await assert.rejects(realHost.sourceScope({...binding,runtimeSha256:'00'.repeat(32)},spec.root,spec.collection),/source scope runtime changed/);
        await assert.rejects(realHost.sourceScope({...binding,controlDir:path.join(fixture.owner,'wrong-control')},spec.root,spec.collection),/source scope control changed/);

        const invoke=(RuntimeHost.prototype as any).invoke;
        editor=await serviceTestHost(context,{bundledVersion:version,runtimeVersion:version,installedVersion:version,
            automatic:false,assistance:false,nativeExecution:true,
            nativeSourceScope:{binding,source:spec.root,collection:spec.collection}});
        assert.equal((RuntimeHost.prototype as any).invoke,invoke,'real NativeRunner/invoke must not be mocked');
        const workbench=editor.views.get('airk.workbench') as any;
        assert.ok(workbench?.host,'actual provider owns actual ServiceExtension host');
        const published=workbench.host.specs().find((value:any)=>value.root===spec.root);
        assert.equal(published?.service.project,spec.service!.project);
        assert.equal(published?.dataDir,spec.dataDir);
        const editorRoot=await fs.realpath(spec.root);
        const editorReceipt=[...editor.registrations.values()].find(row=>row.root===editorRoot);
        assert.ok(editorReceipt,'editor receipt uses its actual canonical opened root');
        assert.notEqual(editorReceipt.binding,spec.service!.project,'fixture editor receipt cannot fake native-selected identity');
        const after=await fixture.run(['runtime','snapshot','--install-dir',fixture.install]);
        assert.equal(record(after.lifecycle).namespace,record(before.lifecycle).namespace);
        // Background owned metadata can legitimately advance epoch/revision.
        // Noncreating/read-only byte proof belongs to the separate native driver.
        assert.equal(direct.registration!.root,spec.root);
        await editor.close();editor=undefined;
        await context.test('Windows real ACL tool success preserves strict native foreign-ACE refusal',
            {skip:process.platform!=='win32',timeout:60000},async()=>{
            const storage=path.join(fixture.owner,'acl-qualified');
            const packageJson=JSON.parse(await fs.readFile(path.join(extension,'package.json'),'utf8'));
            // Actual default async fixed-system whoami/icacls; no runner replacement.
            await writeLaunchDescriptor(storage,extension,packageJson.version,{path:process.execPath,electron:false});
            const descriptor=launchDescriptorPath(storage),entry=assistanceEntry(spec,descriptor)!;
            assert.equal((await probeAssistance(entry,spec.root,version)).version,version);
            const directory=assistanceDirectory(storage),system=process.env.SystemRoot;
            assert.ok(system&&path.isAbsolute(system),'actual Windows system directory required');
            const icacls=path.join(system!,'System32','icacls.exe');
            // Harmless read access on this owned child only. Keep current-user
            // cleanup rights; do not deny self or change any external ACL.
            await promisify(execFile)(icacls,[directory,'/grant','*S-1-1-0:(OI)(CI)R'],{windowsHide:true,timeout:10000,maxBuffer:128*1024});
            // Existing helper grants do not remove unrelated explicit ACEs; it
            // must not be mislabeled as a DACL auditor/rejection mechanism.
            await writeLaunchDescriptor(storage,extension,packageJson.version,{path:process.execPath,electron:false});
            await assert.rejects(runText(fixture.binary,['explorer-assistance','--descriptor',descriptor],fixture.owner,15000),/explorer_assistance_descriptor_invalid/);
            const descriptorJson=JSON.parse(await fs.readFile(descriptor,'utf8'));
            assert.equal(descriptorJson.server_sha256.length,64);
        });
    }finally{
        try{await editor?.close();}finally{await fixture.dispose();}
    }
});
