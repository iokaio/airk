// SPDX-License-Identifier: Apache-2.0
'use strict';
// Consume original Rust-emitted bytes, never define a parallel budget/default.
const fs=require('node:fs'),crypto=require('node:crypto'),path=require('node:path');
function readNativeDelivery(pin,policy){
 if(!pin||!path.isAbsolute(pin.path)||!/^[a-f0-9]{64}$/.test(pin.sha256||''))throw Error('pinned native delivery fixture required');
 const stat=fs.lstatSync(pin.path);if(!stat.isFile()||stat.isSymbolicLink()||stat.size>16777216)throw Error('ordinary bounded native delivery fixture required');
 const raw=fs.readFileSync(pin.path);if(crypto.createHash('sha256').update(raw).digest('hex')!==pin.sha256)throw Error('native delivery fixture bytes differ');
 const value=JSON.parse(raw);
 if(value.schema!=='airk.arbiter.budget-cases/1'||value.operation_deadline_ms!==policy.deadline_ms||value.default_budget.input_tokens!==policy.shared_input_tokens||!Number.isSafeInteger(value.transport_deadline_ms)||value.transport_deadline_ms<policy.deadline_ms)throw Error('native delivery/policy sources differ');
 return {raw,deadlineMs:value.transport_deadline_ms};
}
module.exports={readNativeDelivery};
