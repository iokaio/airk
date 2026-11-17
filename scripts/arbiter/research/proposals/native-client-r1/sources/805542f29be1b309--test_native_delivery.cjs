// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {readNativeDelivery}=require('./helpers/native_delivery.cjs');
test('consume real Rust-emitted transport allowance and refuse changed native sources',()=>{
 const packet=JSON.parse(fs.readFileSync(path.join(__dirname,'preparation.json')));
 const pin=packet.native_budget_cases;
 const policy={deadline_ms:120000,shared_input_tokens:8192}; // literal independent test expectations
 assert.equal(readNativeDelivery(pin,policy).deadlineMs,125000);
 assert.throws(()=>readNativeDelivery({...pin,sha256:'0'.repeat(64)},policy));
 assert.throws(()=>readNativeDelivery(pin,{...policy,deadline_ms:30000}));
});
