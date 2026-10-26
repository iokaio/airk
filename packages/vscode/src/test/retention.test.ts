// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { RetentionStatus, retentionSummary } from '../retention';
import { serviceTestHost } from './serviceTestHost';

test('retention status distinguishes unknown bytes, pending work and failed cleanup',()=>{
    const status:RetentionStatus={collection:'original',policy:{policy:{version:1,mode:'manual',searchable_predecessors:1},origin:'existing_manual',digest:'fixture'},protected_bytes:null,pending_intents:2,pending_bytes:null,last_maintenance:{error:'original cleanup failure'}};
    assert.match(retentionSummary(status),/manual \(existing_manual\)/);assert.match(retentionSummary(status),/unknown protected bytes/);
    assert.match(retentionSummary(status),/2 pending maintenance intents/);assert.match(retentionSummary(status),/maintenance error/);
    status.legacy_protected=[{revision:'original-legacy-digest',bytes:null,reasons:['predecessor']}];status.pressure={required_bytes:200,available_bytes:100};
    assert.match(retentionSummary(status),/1 protected legacy revisions/);assert.match(retentionSummary(status),/200 needed, 100 available/);
    assert.equal(retentionSummary(),'Retention: not inspected');
});

test('current maintenance captures a native guarded preview and policy commands cannot invoke legacy writers',async context=>{
    const f=await serviceTestHost(context);
    try{await f.commands.get('airk.currentMaintenance')!();const request=f.writes.find(value=>value.plan_create).plan_create;
        assert.equal(request.request.operation,'auto');assert.deepEqual(request.request.selection,{selected:[[...f.registrations.keys()][0]]});
        assert.equal(f.writes.some(value=>value.plan_execute),false);
        for(const command of ['airk.configureExtractionPolicy','airk.storageLimits'])await f.commands.get(command)!();
        assert.equal(f.writes.some(value=>value.configure_project||value.configure_resources),false);
        for(const retired of ['airk.retention','airk.retentionPolicy','airk.retentionRun','airk.history','airk.checkpoints','airk.prune'])assert.equal(f.commands.has(retired),false);
    }finally{await f.close();}
});

test('compact storage command previews only the active workspace project and respects trust',async context=>{
    const f=await serviceTestHost(context,{roots:2});
    try{
        f.stub.window.activeTextEditor={document:{uri:f.folders[1].uri}};
        await f.commands.get('airk.compactStorage')!();
        const requests=f.writes.filter(value=>value.plan_create);
        assert.equal(requests.length,1);
        assert.equal(requests[0].plan_create.request.operation,'compact');
        const project=[...f.registrations.values()].find(value=>value.root===f.folders[1].uri.fsPath)!;
        assert.deepEqual(requests[0].plan_create.request.selection,{selected:[project.binding]});
        assert.equal(f.writes.some(value=>value.plan_execute),false);
        f.stub.workspace.isTrusted=false;
        await assert.rejects(f.commands.get('airk.compactStorage')!(),/runtime_client_no_longer_authorized/);
        assert.equal(f.writes.filter(value=>value.plan_create).length,1);
    }finally{await f.close();}
});
