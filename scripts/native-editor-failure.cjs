// SPDX-License-Identifier: Apache-2.0
const fs=require('node:fs/promises'),path=require('node:path');
const {createHash,randomBytes}=require('node:crypto');
const {isDeepStrictEqual}=require('node:util');
const {execFile}=require('node:child_process');
const requireValue=(condition,message)=>{if(!condition)throw Error(message);};
const hex=(value,length)=>typeof value==='string'&&new RegExp(`^[a-f0-9]{${length}}$`).test(value);
const normalize=value=>process.platform==='win32'?path.toNamespacedPath(value).toLowerCase():value;
const same=(a,b)=>typeof a==='string'&&typeof b==='string'&&normalize(a)===normalize(b);
async function canonical(value){
    requireValue(typeof value==='string'&&path.isAbsolute(value),'Failure Stop path is not absolute');
    const resolved=await fs.realpath(value);
    requireValue(same(resolved,path.resolve(value)),'Failure Stop path is redirected');return resolved;
}
async function exclusive(file,value){
    const output=await fs.open(file,'wx');
    try{await output.writeFile(JSON.stringify(value,null,2)+'\n');await output.sync();}finally{await output.close();}
}
function completeReceipt(response,request){
    const {force,...expected}=request;
    requireValue(force===false&&response?.status==='lifecycle'&&response.record?.phase==='complete'
        &&hex(response.record.id,64)&&isDeepStrictEqual(response.record.request,{...expected,manager:false}),
    'Failure Stop response is incomplete or crossed');
}
function invokeNativeJson(command,args,{timeout,input,environment,record}){
    requireValue(input===undefined||Buffer.byteLength(input)<=65536,'Failure Stop native input exceeds 64 KiB');
    requireValue(Number.isSafeInteger(timeout)&&timeout>0&&timeout<=30000,'Failure Stop native deadline exceeds the driver budget');
    return new Promise((resolve,reject)=>{
        const child=execFile(command,args,{cwd:path.dirname(command),env:environment,windowsHide:true,timeout,maxBuffer:65536},(error,stdout,stderr)=>{
            Object.assign(record,{stdout,stderr,exit_code:error?.code??0,signal:error?.signal??null,killed:error?.killed??false,
                truncated:error?.code==='ERR_CHILD_PROCESS_STDIO_MAXBUFFER',outcome_unknown:!!error});
            if(error){reject(error);return;}
            try{resolve(JSON.parse(stdout));}catch(parseError){record.outcome_unknown=true;record.parse_error=String(parseError);reject(parseError);}
        });
        child.stdin.on('error',()=>{}); // The single child's bounded exit owns the outcome.
        child.stdin.end(input);
    });
}
exports.invokeNativeJson=invokeNativeJson;
// Only a fixture already admitted by the installed driver may enter this owner.
// Reads never start a service. A durable exclusive intent precedes one Stop;
// uncertainty retains the intent and response and is never replayed.
async function stopOwnedFixture({fixture,install,control,bundle,invoke,normalStop=null,editorsClosed}){
    const evidence={acceptance:false,qualification:'failure_disposal_only',started:Date.now(),calls:[]};
    let owned;
    const call=async(args,input,timeout=15000)=>{
        const record=input===undefined?{args}:{args,input};evidence.calls.push(record);
        try{return record.response=await invoke(bundle.main,args,timeout,input===undefined?undefined:JSON.stringify(input),record);}
        catch(error){record.error=String(error);if(error.stdout!==undefined)record.stdout=error.stdout;if(error.stderr!==undefined)record.stderr=error.stderr;throw error;}
    };
    try{
        requireValue(editorsClosed===true,'Owned editor closure is unconfirmed');
        owned=await canonical(fixture);install=await canonical(install);control=await canonical(control);
        const within=value=>{const relative=path.relative(owned,value);return relative!==''&&!relative.startsWith('..'+path.sep)&&relative!=='..'&&!path.isAbsolute(relative);};
        requireValue(within(install)&&within(control),'Failure Stop crossed its owned fixture');
        requireValue(hex(bundle.digest,64),'Failure Stop runtime digest is unavailable');
        await canonical(bundle.main);await canonical(bundle.source);
        const metadata=JSON.parse(await fs.readFile(path.join(bundle.source,'airk-runtime.json'),'utf8'));
        const executable=await canonical(path.join(install,'runtimes',bundle.digest,path.basename(bundle.main)));
        const hash=async file=>createHash('sha256').update(await fs.readFile(file)).digest('hex');
        requireValue(hex(metadata.airk_sha256,64)&&await hash(executable)===metadata.airk_sha256
            &&await hash(bundle.main)===metadata.airk_sha256,'Failure Stop executable identity changed');
        const inspect=async()=>{
            const observation=await call(['runtime','inspect','--install-dir',install]);
            requireValue(observation?.payload_verified===true&&observation.approval?.manifest_sha256===bundle.digest
                &&observation.selection?.active===bundle.digest&&same(observation.executable,executable)
                &&same(observation.control_dir,control)&&hex(observation.registry_namespace,32),
            'Failure Stop inspection crossed its exact installation');return observation;
        };
        const before=await inspect();evidence.owner={fixture:owned,install,control,executable,runtime:bundle.digest,namespace:before.registry_namespace};
        evidence.before=before;
        if(normalStop!==null){
            const capture=normalStop.capture,request=capture?.response?.record?.request;
            requireValue(capture?.qualification==='historical_native_lifecycle_response'&&capture.force===false
                &&same(capture.runtime?.installDir,install)&&same(capture.runtime?.controlDir,control)
                &&capture.runtime?.runtimeSha256===bundle.digest&&capture.namespace===before.registry_namespace
                &&normalStop.requested_binding?.namespace===before.registry_namespace
                &&capture.before_health?.status==='health'&&capture.before_health.observation?.version===bundle.version
                &&capture.before_health.observation?.protocol===1&&capture.before_health.observation?.runtime_sha256===bundle.digest
                &&capture.before_health.observation?.executable_sha256===metadata.airk_sha256
                &&capture.before_health.observation?.catalog_clock?.namespace===before.registry_namespace
                &&Number.isSafeInteger(capture.before_health.observation?.process_id)&&capture.before_health.observation.process_id>0
                &&capture.before_health.observation?.policy?.accepting===true
                &&request?.expected_instance===capture.before_health.observation?.instance
                &&hex(request?.expected_instance,32)&&request?.runtime===bundle.digest&&request?.action==='stop'
                &&request?.key?.namespace===before.registry_namespace&&hex(request?.caller,32)
                &&Number.isSafeInteger(request?.key?.epoch)&&request.key.epoch>=0&&hex(request?.key?.nonce,32)
                &&Number.isSafeInteger(capture.capturedAt)&&capture.capturedAt>=normalStop.started
                &&capture.response.record.id!==normalStop.previous_capture?.response?.record?.id
                &&normalStop.dispatch?.dispatch!==normalStop.confirmation?.dispatch,
            'Existing UI Stop proof is incomplete or crossed');
            const {manager,...input}=request;requireValue(manager===false,'Existing UI Stop manager crossed');
            completeReceipt(capture.response,{...input,force:false});
            requireValue(before.lifecycle?.explicitly_stopped===true&&before.lifecycle.pending===null,'Existing UI Stop has not settled');
            evidence.reused_normal_stop=normalStop;evidence.complete=true;
        }else{
            requireValue(before.lifecycle?.explicitly_stopped===false&&before.lifecycle.pending===null,'Failure Stop owner is already stopped or has uncertain lifecycle work');
            const health=await call(['service','rpc','--runtime-dir',control,'--stdin'],'health');evidence.health=health;
            const observed=health?.observation;
            requireValue(health?.status==='health'&&observed?.version===bundle.version&&observed.protocol===1
                &&observed.runtime_sha256===bundle.digest&&observed.executable_sha256===metadata.airk_sha256
                &&observed.catalog_clock?.namespace===before.registry_namespace&&hex(observed.instance,32)
                &&Number.isSafeInteger(observed.process_id)&&observed.process_id>0&&observed.policy?.accepting===true,
            'Failure Stop authenticated health crossed its runtime or namespace');
            const epoch=await call(['service','rpc','--runtime-dir',control,'--stdin'],'request_epoch');evidence.epoch=epoch;
            requireValue(epoch?.status==='request_epoch'&&epoch.namespace===before.registry_namespace
                &&Number.isSafeInteger(epoch.epoch)&&epoch.epoch>=0,'Failure Stop epoch crossed its namespace');
            const request={caller:randomBytes(16).toString('hex'),key:{namespace:epoch.namespace,epoch:epoch.epoch,nonce:randomBytes(16).toString('hex')},
                action:'stop',expected_instance:observed.instance,runtime:bundle.digest,force:false};
            evidence.request=request;
            const current=await inspect();
            requireValue(current.registry_namespace===before.registry_namespace&&current.lifecycle?.pending===null
                &&current.lifecycle?.explicitly_stopped===false&&await hash(executable)===metadata.airk_sha256,
            'Failure Stop authority changed before dispatch');
            await exclusive(path.join(owned,'failure-native-stop-intent.json'),evidence);
            evidence.dispatched=true;
            evidence.response=await call(['runtime','lifecycle','--install-dir',install],request,30000);
            completeReceipt(evidence.response,request);
            evidence.after=await inspect();
            requireValue(evidence.after.registry_namespace===before.registry_namespace&&evidence.after.lifecycle?.explicitly_stopped===true
                &&evidence.after.lifecycle.pending===null,'Failure Stop has not settled stopped');
            evidence.complete=true;
        }
        await exclusive(path.join(owned,'failure-native-stop.json'),evidence);return evidence;
    }catch(error){
        evidence.error=String(error);evidence.complete=false;
        if(owned){try{await exclusive(path.join(owned,'failure-native-stop-error.json'),evidence);}
            catch(retentionError){throw new AggregateError([error,retentionError],'Failure Stop evidence could not be retained');}}
        throw error;
    }
}
exports.stopOwnedFixture=stopOwnedFixture;
// The injected client probe precedes the ordinary installed-driver Stop owner.
exports.withFailureStop=async({run,stop,publishStop,retainFailure})=>{
    try{return await run();}
    catch(error){
        const evidence={client_error:String(error),stop_attempted:true,stop_receipt_published:false};
        try{
            // Exactly one mutation dispatch. An uncertain Stop or failed receipt
            // write remains incomplete; never restart or repeat it for evidence.
            const receipt=await stop();
            await publishStop(receipt);
            evidence.stop_receipt_published=true;
        }catch(cleanupError){evidence.cleanup_error=String(cleanupError);}
        try{await retainFailure(evidence);}
        catch(retentionError){throw new AggregateError([error,retentionError],'Client failed and cleanup evidence could not be retained');}
        throw error;
    }
};
