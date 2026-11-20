// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {timing,ARMS} = require('./exchanges.cjs');
const ids=['q97','q98','q99','q100'];
const plan={schema:'airk.private.cal-phase12-affinity-timing/1',question_ids:ids};
function setup(pass, override={}) {
    const log=[]; let tick=100;
    const m=timing(plan,pass,77,ids,'service',()=>99,e=>log.push(e),(ordinal,arm)=>({
        pid:77,ordinal,pass_index:pass,arm,success:true,full_mask_hex:'0xff',observed_mask_hex:arm==='full'?'0xff':'0x55',
        ...override}),()=>({unix_ms:++tick,monotonic_ns:String(tick)}));
    return [m,log];
}
test('eight engineered questions retain real ordered seven-call deliveries and distinct grants',async()=>{
    for (const pass of [0,1]) {
        const [m,log]=setup(pass);
        for (const [i,id] of ids.entries()) {
            m.begin(id); m.bound({scope_identity:`grant-${pass}-${i}`});
            for (const tool of ['status','answer','context','search','context','search','search']) {
                const result={exitCode:0,class:'ok',stdout:'original',stderr:''};
                const session={async invoke(argv,opts){assert.equal(argv[2],tool);assert.equal(opts.deadlineMs,125000);return result;}};
                assert.equal(await m.invoke(session,['tools','call',tool],{deadlineMs:125000}),result);
            }
            assert.throws(()=>m.complete({id,probes:[]}));
            m.complete({id,probes:Array(5).fill({})});
            m.end(id);
        }
        m.finish();
        const rows=log.filter(x=>x.kind==='timing_exchange');
        assert.equal(rows.length,28);
        assert.deepEqual(log.filter(x=>x.kind==='timing_assignment').map(x=>x.arm),ARMS[pass]);
        assert.equal(new Set(rows.map(x=>x.scope)).size,4);
        assert.ok(rows.every(x=>x.cli_pid===99&&x.service_pid===77&&x.native_delivery&&x.end.unix_ms>x.start.unix_ms));
    }
});
test('natural failure retained, no retry and no complete question invented',async()=>{
    const [m,log]=setup(0);
    m.begin(ids[0]);m.bound({scope_identity:'grant'});
    let calls=0;
    const failure=Error('transport refused');
    await assert.rejects(m.invoke({async invoke(){calls++;throw failure;}},['tools','call','status'],{deadlineMs:125000}),
                         e=>e===failure);
    assert.equal(calls,1);
    assert.equal(log.at(-1).error,'transport refused');
    assert.equal(log.at(-1).stdout_sha256,null);
    assert.throws(()=>m.finish());
});
test('wrong affinity receipt, repeated ordinal or shorter deadline are rejected',async()=>{
    assert.throws(()=>setup(0,{pid:89})[0].begin(ids[0]));
    assert.throws(()=>setup(0,{observed_mask_hex:'0x4'})[0].begin(ids[0]));
    const [m]=setup(0);m.begin(ids[0]);assert.throws(()=>m.begin(ids[0]));
    m.bound({scope_identity:'grant'});
    await assert.rejects(m.invoke({},['tools','call','answer'],{deadlineMs:1000}),/unbounded/);
});
