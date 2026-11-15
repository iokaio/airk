// SPDX-License-Identifier: Apache-2.0
// Pure harness admission controls, not native transport qualification.
'use strict';
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const {test}=require('node:test');
const root=path.resolve(__dirname,'../..');
const source=fs.readFileSync(path.join(root,'scripts/workbench-window-scope.cjs'),'utf8');
const body=source.slice(source.indexOf('function qualifiedAnswerPartitions('),source.indexOf('async function runAnswer('));
const {answerPartitions}=require(path.join(root,'packages/vscode/out/workbenchAnswerNavigation'));
const context=vm.createContext({assert,answerPartitions});vm.runInContext(body,context);
const qualify=value=>context.qualifiedAnswerPartitions(value);
const direct=JSON.parse(fs.readFileSync(path.join(root,'fixtures/split/arbiter/context_action_cases.json'),'utf8')).cases.find(row=>row.accepted&&row.input?.records?.length).input;
const full=JSON.parse(fs.readFileSync(path.join(root,'fixtures/split/arbiter/combined_answer_cases.json'),'utf8')).cases.find(row=>row.name==='two_complete_members').output;
// Synthetic one-index routing wrapper retains the unchanged actual Rust packet.
const singleCombined={...full,members:full.members.slice(0,1)};
test('current singleton context uses local outcome and preserves complete packet',()=>{
 const before=JSON.stringify(direct),parts=qualify(direct);
 assert.equal(parts.length,1);assert.equal(parts[0].outcome,'local');
 assert.deepEqual(parts[0].packet,direct);assert.equal(JSON.stringify(direct),before);
});
test('current combined singleton uses complete outcome and preserves original member',()=>{
 const before=JSON.stringify(singleCombined),parts=qualify(singleCombined);
 assert.equal(parts.length,1);assert.equal(parts[0].outcome,'complete');
 assert.deepEqual(parts[0].packet,singleCombined.members[0].result);
 assert.equal(JSON.stringify(singleCombined),before);
});
test('future schemas, missing combined context, failed members and extra indexes refuse',()=>{
 for(const bad of [{...direct,schema:'airk.arbiter.context/future'},
                  {...singleCombined,context:undefined},
                  {...singleCombined,members:[{...singleCombined.members[0],outcome:'failed'}]},full]){
  assert.throws(()=>qualify(bad));
 }
});

test('failed source stage retains complete already observed native error and command without normalization or replay',()=>{
 const functionBody=source.slice(source.indexOf('function failureObservation('),source.indexOf('async function until('));
 const failureContext=vm.createContext({});vm.runInContext(functionBody,failureContext);
 const nativeResponse={isError:true,content:[{type:'text',text:JSON.stringify({error:{code:'actual_native_code',retryable:false,future:['every','field']}})}]};
 const error=new Error('public sanitized guidance');error.nativeResponse=nativeResponse;error.nativePayload={error:{code:'actual_native_code'},future:{kept:true}};
 const deliveries=[{argv:['tools','call','airk_file_info','--args','{"revision":"exact"}'],result:{stdout:JSON.stringify(nativeResponse),stderr:'',code:0}}];
 const before=JSON.stringify(deliveries),result=failureContext.failureObservation(error,'answer-window-file-info-and-open',deliveries,[]);
 assert.equal(result.stage,'answer-window-file-info-and-open');assert.deepEqual(result.error.nativeResponse,nativeResponse);
 assert.deepEqual(result.error.nativePayload,error.nativePayload);assert.equal(result.deliveries,deliveries);assert.equal(JSON.stringify(deliveries),before);
});
