// SPDX-License-Identifier: Apache-2.0
const fs=require('node:fs/promises'),path=require('node:path'),repo=path.resolve(__dirname,'../../..');
const {privateEditorEnvironment,privateClientEnvironment}=require(path.join(repo,'scripts/editor-ui.cjs'));
const {RuntimeHost,nativeRunner}=require(path.join(repo,'packages/vscode/out/serviceBootstrap.js'));
(async()=>{
 const fixture=path.join(repo,'artifacts/editor-qualification/native-stage-Mye22Y'),folder=path.join(fixture,'multi'),install=path.join(folder,'native-service');
 const env=privateClientEnvironment(privateEditorEnvironment(process.env),path.join(fixture,'client-home'));
 for(const [key,value]of Object.entries(env))process.env[key]=value;
 const host=new RuntimeHost(path.join(__dirname,'windows-debug-c8-stage-r2/stage'),'1.8.14',{channel:'stable',installDir:install},nativeRunner,()=>true);
 const found=await host.discover();if(!found.inspection)throw Error('Missing exact retained installation');
 const binding=host.binding(found.inspection,found.installDir),client=host.client(binding);
 try{
  const health=await client.rpc('health','health');
  if(health.observation.instance!=='b138fd1cdf13edcb17f4853f5c9cf954'||binding.runtimeSha256!=='66df92b5309ef1f8af8f29c28b1c2e06130fa83284c8b97bcfab3cf4c5258631')throw Error('Retained fixture identity changed');
  const prepared=await client.prepareJob('256dc7920f2ff535c3c62e82ad5b12717b158258299c36930641c6c48121ebc8','inspect');
  const intent={lifecycle:{caller:prepared.enqueue.caller,key:prepared.enqueue.key,action:'stop',expected_instance:health.observation.instance,runtime:binding.runtimeSha256}};
  await fs.writeFile(path.join(folder,'diagnostic-stop-intent.json'),JSON.stringify({qualification:'explicit diagnostic cleanup; not UI acceptance',health,intent},null,2),{flag:'wx'});
  const receipt=await host.lifecycle(binding,intent,false);
  await fs.writeFile(path.join(folder,'diagnostic-stop-receipt.json'),JSON.stringify(receipt,null,2),{flag:'wx'});
  console.log(JSON.stringify({phase:receipt.record.phase,action:receipt.record.request.action,instance:health.observation.instance}));
 }finally{client.dispose();}
})().catch(error=>{console.error(error);process.exitCode=1;});
