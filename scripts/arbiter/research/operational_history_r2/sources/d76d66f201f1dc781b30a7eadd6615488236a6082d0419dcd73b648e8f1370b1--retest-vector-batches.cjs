// SPDX-License-Identifier: Apache-2.0
// Isolated, single-shot candidate retest; no normal workspace installation.
const fs=require('node:fs/promises'),path=require('node:path'),assert=require('node:assert/strict');
const {NativeServiceFixture}=require(path.resolve('packages/vscode/out/test/nativeServiceFixture.js'));
const {McpStdioSession}=require(path.resolve('packages/vscode/out/mcpStdioSession.js'));
const {launchArgs}=require(path.resolve('packages/vscode/out/model.js'));
const output=path.resolve('artifacts/mcp-optimizations-1.8.4/vector-mixed-batches.json');
const body=result=>result.structuredContent??JSON.parse(result.content[0].text);
async function main(){
 const fixture=await NativeServiceFixture.create(path.resolve('artifacts/mcp-optimizations-1.8.4/runtime/airk.exe'));
 const sessions=[],report={version:'1.8.4',basis:'Managed installation of fresh candidate into an isolated fictional fixture; no automatic retries',batches:[],runtime:[],passed:false,cleanup:false};
 try{
  const spec=await fixture.register('vector-mixed-status');
  await fs.writeFile(path.join(spec.root,'session.md'),'# Session\nSession is a fictional local handle for request processing.\nA Session supports scoped evidence reads.\n');
  await fs.writeFile(path.join(spec.root,'session.rs'),'pub struct Session;\nimpl Session { pub fn new() -> Self { Session } }\n');
  await fixture.job(spec,'model_setup');await fixture.job(spec);await fixture.ready(spec,2);
  for(const binding of ['source','pinned']){
   const args=binding==='pinned'?launchArgs(spec,'mcp',true):['runtime','launch','--install-dir',fixture.install,'--','--root',spec.root,'mcp'];
   const session=(await McpStdioSession.open({command:fixture.launcher,args,cwd:fixture.owner,clientName:'airk-184-vector-retest',clientVersion:'1.8.4'},60000)).session;
   sessions.push(session);
   const projects=body(await session.request('tools/call',{name:'projects',arguments:{}},10000));
   const project=projects.default_project;
   const before=body(await session.request('tools/call',{name:'diagnostics',arguments:{project,response_mode:'full'}},60000));
   report.runtime.push({binding,phase:'before',runtime:before.runtime});
   for(let round=1;round<=3;round++){
    const calls=[['status',{project,response_mode:'full'}],...['lexical','vector','hybrid'].map(mode=>['search',{project,path_prefix:'session.md',query:'Session',mode,language:'markdown',evidence_kind:'document',limit:3,max_bytes:4000}])];
    const results=await Promise.allSettled(calls.map(async([name,arguments_])=>{
     const started=performance.now();const response=await session.request('tools/call',{name,arguments:arguments_},30000);
     return {name,arguments:arguments_,elapsed_ms:performance.now()-started,response};
    }));
    const saved={binding,round,calls:results.map((result,index)=>result.status==='fulfilled'?result.value:{name:calls[index][0],failure:String(result.reason)})};report.batches.push(saved);
    for(const call of saved.calls){
     assert.ok(call.response,JSON.stringify(call));assert.notEqual(call.response.isError,true,JSON.stringify(call));
     const value=body(call.response);assert.equal(value.publication.token,before.publication.token,JSON.stringify(value));
     if(call.name==='status'){assert.equal(value.version,'1.8.4');assert.equal(value.index_state,'available',JSON.stringify(value));}
     else {assert.ok(value.hits.length>0,JSON.stringify(value));assert.ok(value.hits.every(hit=>hit.citation.path==='session.md'),JSON.stringify(value));}
    }
   }
   const after=body(await session.request('tools/call',{name:'diagnostics',arguments:{project,response_mode:'full'}},60000));
   report.runtime.push({binding,phase:'after',runtime:after.runtime});
   assert.equal(after.runtime.service.instance,before.runtime.service.instance);
   assert.deepEqual(after.runtime.index_worker.identity,before.runtime.index_worker.identity);
  }
  report.passed=true;
 }finally{
  for(const session of sessions)session.close();
  try{await fixture.dispose();report.cleanup=true;}finally{await fs.writeFile(output,JSON.stringify(report,null,2)+'\n');}
 }
 const calls=report.batches.flatMap(batch=>batch.calls);
 const notes=calls.filter(call=>call.name==='status'&&body(call.response).status_observation?.superseded===true).length;
 console.log(JSON.stringify({passed:report.passed,cleanup:report.cleanup,batches:report.batches.length,calls:calls.length,status_supersession_notes:notes}));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
