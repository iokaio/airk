// SPDX-License-Identifier: Apache-2.0
// Local validation only; not shipped in the extension.
const path=require('node:path');
const fs=require('node:fs/promises');
const {performance}=require('node:perf_hooks');
const base=path.resolve('packages/vscode/out');
const {NativeServiceFixture}=require(path.join(base,'test/nativeServiceFixture'));
const {McpStdioSession}=require(path.join(base,'mcpStdioSession'));
const {launchArgs}=require(path.join(base,'model'));
(async()=>{
 const f=await NativeServiceFixture.create(path.resolve(process.argv[2]));let session;
 try{
  const spec=await f.register('status-measurement');
  await fs.writeFile(path.join(spec.root,'fixture.rs'),'pub fn status_measurement() -> bool { true }\n');
  await f.configure(spec,{operation:'vectors',enabled:false});await f.job(spec);
  session=(await McpStdioSession.open({command:spec.command,args:launchArgs(spec),cwd:f.owner,clientName:'codex-status-measurement',clientVersion:'1.6.1'},60000)).session;
  const samples=[];
  for(const name of ['status','status','status','diagnostics','status']){
   const started=performance.now();const response=await session.request('tools/call',{name,arguments:{}},60000);
   if(response.isError)throw new Error(JSON.stringify(response));const value=JSON.parse(response.content[0].text);
   samples.push({tool:name,elapsed_ms:+(performance.now()-started).toFixed(2),payload_bytes:Buffer.byteLength(response.content[0].text),wire_result_bytes:Buffer.byteLength(JSON.stringify(response)),state:value.index_state,files:value.files,version:value.version,health:value.health,cached:value.cached,cache_age_ms:value.cache_age_ms,includes_runtime:!!value.runtime,includes_work:!!value.work,includes_editor:!!value.explorer});
  }
  await fs.writeFile(path.resolve(process.argv[3]),JSON.stringify({schema:1,samples},null,2)+'\n');console.log(JSON.stringify(samples));
 }finally{session?.close();await f.dispose();}
})().catch(error=>{console.error(error);process.exitCode=1});
