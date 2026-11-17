// SPDX-License-Identifier: Apache-2.0
const fs=require('node:fs/promises'),path=require('node:path'),{promisify}=require('node:util'),{execFile}=require('node:child_process');
const run=promisify(execFile),repo=path.resolve(__dirname,'../../..');
const {privateEditorEnvironment,privateClientEnvironment}=require(path.join(repo,'scripts/editor-ui.cjs'));
(async()=>{
 const base=path.join(__dirname,'c8-activation-diagnostic-r1');await fs.mkdir(base);
 const home=path.join(base,'client-home');await fs.mkdir(home);
 const env=privateClientEnvironment(privateEditorEnvironment(process.env),home);
 const binary=path.join(__dirname,'windows-debug-r21/runtime/airk.exe'),source=path.join(__dirname,'windows-debug-c8-stage-r1/stage/native/runtime'),install=path.join(base,'native-service');
 const digest='66df92b5309ef1f8af8f29c28b1c2e06130fa83284c8b97bcfab3cf4c5258631';
 for(const [name,args] of [['stage',['runtime','stage','--source',source,'--install-dir',install,'--approved-sha256',digest,'--verification-basis','trusted-distribution','--channel','stable']],['activate',['runtime','activate','--install-dir',install,'--runtime-sha256',digest]]]){
  const started=Date.now();let result;
  try{result={...(await run(binary,args,{env,windowsHide:true,timeout:180000,maxBuffer:1048576})),exit_code:0};}
  catch(error){result={exit_code:error.code,stdout:error.stdout,stderr:error.stderr,error:String(error)};}
  result.arguments=args;result.elapsed_ms=Date.now()-started;await fs.writeFile(path.join(base,name+'.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));if(result.exit_code!==0)break;
 }
})().catch(error=>{console.error(error);process.exitCode=1;});
