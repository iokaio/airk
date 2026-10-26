// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import {record} from '../serviceClient';
import {NativeServiceFixture} from './nativeServiceFixture';

const binary=process.env.AIRK_NATIVE_TEST_BINARY||process.env.AIRK_TEST_BINARY;

test('native console inspections and guarded repair preserve paused, missing and isolated project evidence',
    {skip:!binary,timeout:480000},async()=>{
    const fixture=await NativeServiceFixture.create(binary!);
    try{
        const a=await fixture.register('diagnostic-a'),b=await fixture.register('diagnostic-b');
        const original='pub fn native_diagnostic_probe() -> bool { true }\n';
        await fs.writeFile(path.join(a.root,'original.rs'),original);
        await fs.writeFile(path.join(b.root,'other.rs'),'pub fn unrelated_probe() {}\n');
        await fixture.configure(a,{operation:'vectors',enabled:false});
        await fixture.configure(b,{operation:'vectors',enabled:false});
        await fixture.job(a);await fixture.job(b);
        const first=await fixture.publishedStatus(a),other=await fixture.publishedStatus(b);
        await fixture.client!.rpc({pause:{project:a.service!.project,paused:true}});
        await fixture.job(a,'inspect');
        const inspection=await fixture.client!.rpc({inspection:{project:a.service!.project}},'inspection');
        assert.equal(inspection.project,a.service!.project);
        assert.equal(record(record(inspection.observation).snapshot).source_state,'available');
        assert.ok(!JSON.stringify(inspection).includes('pub fn native_diagnostic_probe'));
        await fixture.client!.rpc({pause:{project:a.service!.project,paused:false}});
        await fixture.repair(a,'verify');
        assert.deepEqual((await fixture.publishedStatus(a)).publication,first.publication);
        const offline=path.join(fixture.owner,'offline-a');await fs.rename(a.root,offline);
        try{
            const retained=await fixture.tool(a,'airk_read_file',{path:'original.rs',start_line:1,end_line:1});
            assert.ok(Array.isArray(retained.hits));assert.equal(record(retained.hits[0]).text,original);
            const accepted=await fixture.client!.submitJob(await fixture.client!.prepareJob(a.service!.project,'index'));
            const failed=await fixture.waitJob(String(record(accepted.job).id));
            assert.equal(failed.state,'failed');assert.equal(record(failed.outcome).code,'source_unavailable');
            await fixture.job(a,'inspect');
            const missing=await fixture.client!.rpc({inspection:{project:a.service!.project}},'inspection');
            assert.equal(record(record(missing.observation).snapshot).source_state,'unavailable');
            assert.deepEqual((await fixture.publishedStatus(a)).publication,first.publication);
        }finally{await fs.rename(offline,a.root);}
        await fixture.repair(a,'rebuild');
        const rebuilt=await fixture.publishedStatus(a);
        assert.notEqual(record(rebuilt.publication).incarnation,record(first.publication).incarnation);
        assert.deepEqual((await fixture.publishedStatus(b)).publication,other.publication);
        const retained=await fixture.tool(a,'airk_read_file',{path:'original.rs',start_line:1,end_line:1});
        assert.ok(Array.isArray(retained.hits));assert.equal(record(retained.hits[0]).text,original);
        assert.equal(await fs.readFile(path.join(a.root,'original.rs'),'utf8'),original);
        assert.equal(await fs.readFile(path.join(b.root,'other.rs'),'utf8'),'pub fn unrelated_probe() {}\n');
    }finally{await fixture.dispose();}
});
