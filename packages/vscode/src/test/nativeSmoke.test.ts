// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {probeMcp} from '../native';
import {record} from '../serviceClient';
import {NativeServiceFixture} from './nativeServiceFixture';

test('registered native MCP verifies before the first publication without starting inference', {skip:!process.env.AIRK_TEST_BINARY,timeout:240000}, async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    try{
        const spec=await fixture.register('unindexed');
        const result=await probeMcp(spec);assert.equal(result.tools.length,16);assert.equal(record(result.status).state,'index_unavailable');
        const status=JSON.parse((record(result.status).content as {text:string}[])[0].text);assert.equal(status.index_state,'unavailable');assert.equal(status.files,null);
        const embedding=await fixture.embedding();
        assert.equal(embedding.loads,0);assert.equal(embedding.sessions,0);
    }finally{await fixture.dispose();}
});
