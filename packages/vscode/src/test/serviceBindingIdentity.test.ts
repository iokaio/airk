// SPDX-License-Identifier: Apache-2.0
// R39 `workspace_registration_changed` binding-identity regression seam
// (docs/research/installer-stop-2026-11-09.md "Current unresolved failure";
// docs/simplify-plan.md R3/S1.2). The real ServiceExtension credential-source
// scan behind provideDefinitions -> hasManualVscodeConnection is paused at its
// existing global-inventory boundary, and refreshes are ordered with deferred
// gates. No fixed sleeps; no production behavior is changed by this file.
import {test,type TestContext} from 'node:test';
import {strict as assert} from 'node:assert';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import {serviceTestHost} from './serviceTestHost';
import {RuntimeHost,runtimeBindingIdentity,type RuntimeBinding} from '../serviceBootstrap';
import {ServiceError} from '../serviceClient';
import * as globalInventory from '../globalAgentInventory';

type Host=Awaited<ReturnType<typeof serviceTestHost>>;
type Outcome={value:unknown[]}|{error:unknown};
// Known failure until the installed-editor repair is authorized; see the stop record.
const R39_TODO='owner: maintainer (D10); reason: R39 workspace_registration_changed equivalent-binding race; repair not authorized; expiry: at D10 decision';

function deferred(){let resolve!:()=>void;const promise=new Promise<void>(done=>{resolve=done;});return {promise,resolve};}
function settle(promise:Promise<unknown[]>):Promise<Outcome>{return promise.then(value=>({value}),error=>({error}));}
function callerStack():string{const limit=Error.stackTraceLimit;Error.stackTraceLimit=64;try{return new Error().stack??'';}finally{Error.stackTraceLimit=limit;}}
const extension=(f:Host):any=>f.context.subscriptions.find((item:any)=>typeof item.provideDefinitions==='function'&&typeof item.resolveAgentSpec==='function');
const definitionValue=(definition:any)=>[definition.label,definition.uri.toString()];

/** Pause only the next global inventory read made by vscodeCredentialSources.
 * Other inventory callers (for example background HTTP configuration hints)
 * keep the fixture's empty inventory and are never paused. */
function pauseCredentialScan(){
    const entered=deferred(),release=deferred();let armed=true;
    const inventory=globalInventory.inventoryGlobalAgentConfigs as unknown as {mock:{mockImplementation(fn:(...args:unknown[])=>Promise<unknown>):void}};
    inventory.mock.mockImplementation(async()=>{
        if(armed&&callerStack().includes('vscodeCredentialSources')){armed=false;entered.resolve();await release.promise;}
        return {files:[],checked:[],blockers:[]};
    });
    return {entered:entered.promise,release:release.resolve};
}
/** Pause the next connection discovery, i.e. the start of a refreshOnce. */
function pauseConnectionDiscovery(){
    const entered=deferred(),release=deferred();let armed=true;
    const discover=RuntimeHost.prototype.discoverForConnection as unknown as {mock:{mockImplementation(fn:(this:RuntimeHost)=>Promise<unknown>):void}};
    discover.mock.mockImplementation(async function(this:RuntimeHost){if(armed){armed=false;entered.resolve();await release.promise;}return this.discover();});
    return {entered:entered.promise,release:release.resolve};
}
const staleScan=(error:unknown)=>{
    assert.ok(error instanceof ServiceError,String(error));assert.equal(error.code,'workspace_registration_changed');
    assert.match(error.stack??'',/vscodeCredentialSources/,'refused by the credential-source scan stale-binding check');return true;
};

/** Provider enters the credential scan, `change` runs (and must complete a
 * refresh) while the scan is paused, then the scan is released. */
async function scanAcrossRefresh(t:TestContext,change:(f:Host,controller:any)=>Promise<void>,inspect:(before:{binding:RuntimeBinding;generation:number},controller:any)=>void):Promise<{baseline:unknown[];outcome:Outcome}>{
    const f=await serviceTestHost(t),controller=extension(f);let scan:ReturnType<typeof pauseCredentialScan>|undefined,provided:Promise<unknown[]>|undefined;
    try{
        const baseline=(await controller.provideDefinitions()).map(definitionValue);assert.equal(baseline.length,1,'fixture publishes one automatic definition without a race');
        const before={binding:controller.binding as RuntimeBinding,generation:controller.generation as number};assert.ok(before.binding);
        let fired=0;controller.definitions.event(()=>{fired++;});
        scan=pauseCredentialScan();const pending:Promise<unknown[]>=controller.provideDefinitions();provided=pending;pending.catch(()=>{});
        await scan.entered;
        await change(f,controller);
        inspect(before,controller);
        t.diagnostic(`binding object replaced=${controller.binding!==before.binding}; definitions events during refresh=${fired}`);
        scan.release();
        return {baseline,outcome:await settle(pending.then(value=>value.map(definitionValue)))};
    }finally{scan?.release();await provided?.catch(()=>{});await f.close();}
}

test('equivalent binding refresh during the manual-connection credential scan keeps definitions available',{todo:R39_TODO},async t=>{
    const {baseline,outcome}=await scanAcrossRefresh(t,async(_f,controller)=>{await controller.refresh();},(before,controller)=>{
        assert.ok(controller.binding,'refresh published a binding');
        assert.equal(runtimeBindingIdentity(controller.binding),runtimeBindingIdentity(before.binding),'unchanged runtime/control authority');
        assert.equal(controller.generation,before.generation,'unchanged generation');
    });
    if('error' in outcome)throw outcome.error;
    assert.deepEqual(outcome.value,baseline);
});

test('runtime change during the manual-connection credential scan still refuses',async t=>{
    const runtime='cd'.repeat(32);
    const {outcome}=await scanAcrossRefresh(t,async(f,controller)=>{f.binding.runtimeSha256=runtime;await controller.refresh();},(before,controller)=>{
        assert.equal(controller.binding?.runtimeSha256,runtime,'refresh published the changed runtime');
        assert.equal(controller.binding.controlDir,before.binding.controlDir);assert.equal(controller.generation,before.generation);
    });
    assert.ok('error' in outcome,'a changed runtime must not publish definitions from the superseded binding');staleScan(outcome.error);
});

test('control directory change during the manual-connection credential scan still refuses',async t=>{
    const {outcome}=await scanAcrossRefresh(t,async(f,controller)=>{
        const control=path.join(f.root,'replacement-control');await fs.mkdir(path.join(control,'http-1'),{recursive:true,mode:0o700});
        f.binding.controlDir=control;await controller.refresh();
    },(before,controller)=>{
        assert.ok(controller.binding,'refresh published a binding');assert.notEqual(controller.binding.controlDir,before.binding.controlDir,'refresh published the changed control directory');
        assert.equal(controller.binding.runtimeSha256,before.binding.runtimeSha256);assert.equal(controller.generation,before.generation);
    });
    assert.ok('error' in outcome,'a changed control directory must not publish definitions from the superseded binding');staleScan(outcome.error);
});

test('generation change during the manual-connection credential scan still refuses',async t=>{
    const {outcome}=await scanAcrossRefresh(t,async(_f,controller)=>{controller.reset();await controller.refresh();},(before,controller)=>{
        assert.equal(controller.generation,before.generation+1,'reset advanced the generation');
        assert.ok(controller.binding,'refresh published a binding');
        assert.equal(runtimeBindingIdentity(controller.binding),runtimeBindingIdentity(before.binding),'only the generation changed');
    });
    assert.ok('error' in outcome,'a changed generation must not publish definitions from the superseded generation');staleScan(outcome.error);
});

test('provider entering while an equivalent refresh is pending keeps definitions available',{todo:R39_TODO},async t=>{
    const f=await serviceTestHost(t),controller=extension(f);let scan:ReturnType<typeof pauseCredentialScan>|undefined,discovery:ReturnType<typeof pauseConnectionDiscovery>|undefined,provided:Promise<unknown[]>|undefined,refreshing:Promise<void>|undefined;
    try{
        const baseline=(await controller.provideDefinitions()).map(definitionValue);assert.equal(baseline.length,1,'fixture publishes one automatic definition without a race');
        const before={binding:controller.binding as RuntimeBinding,generation:controller.generation as number};assert.ok(before.binding);
        discovery=pauseConnectionDiscovery();let refreshed=false;
        refreshing=controller.refresh() as Promise<void>;refreshing.then(()=>{refreshed=true;},()=>{});
        await discovery.entered;assert.equal(refreshed,false,'refresh is pending when the provider enters');
        scan=pauseCredentialScan();const pending:Promise<unknown[]>=controller.provideDefinitions();provided=pending;pending.catch(()=>{});
        // Release refresh first. Whether the provider joins it or scans
        // concurrently, the scan is still paused until refresh has completed.
        discovery.release();await refreshing;
        assert.ok(controller.binding,'refresh published a binding');
        assert.equal(runtimeBindingIdentity(controller.binding),runtimeBindingIdentity(before.binding),'unchanged runtime/control authority');
        assert.equal(controller.generation,before.generation,'unchanged generation');
        t.diagnostic(`binding object replaced=${controller.binding!==before.binding}`);
        // The current code reaches the scan before refresh completes; a joining
        // provider reaches it only now. An early refusal settles instead.
        await Promise.race([scan.entered,pending.then(()=>undefined,()=>undefined)]);
        scan.release();
        const outcome=await settle(pending.then(value=>value.map(definitionValue)));
        if('error' in outcome)throw outcome.error;
        assert.deepEqual(outcome.value,baseline);
    }finally{discovery?.release();scan?.release();await refreshing?.catch(()=>{});await provided?.catch(()=>{});await f.close();}
});
