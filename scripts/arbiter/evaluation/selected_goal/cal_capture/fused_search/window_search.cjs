// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs=require('node:fs'), path=require('node:path'), crypto=require('node:crypto');
const {spawn}=require('node:child_process');
const {isDeepStrictEqual:equal}=require('node:util');
const {acquire,release,windowStatus}=require('../window_scope.cjs');
function need(ok,message){if(!ok)throw Error(message);}
function read(p){return JSON.parse(fs.readFileSync(p));}
function pinned(ref,json=true){
    const bytes=fs.readFileSync(ref.path);
    need(bytes.length===ref.bytes && crypto.createHash('sha256').update(bytes).digest('hex')===ref.sha256,'pin differs');
    if(json)return JSON.parse(bytes);
}
async function main(){
    const [planPath,planDigest,label]=process.argv.slice(2);
    const plan=pinned({path:planPath,bytes:fs.statSync(planPath).size,sha256:planDigest});
    const audit=pinned(plan.audit), work=path.join(path.dirname(planPath),'fixture');
    const ready=read(path.join(work,'ready.json')), bindings=read(path.join(work,'enable-context.bindings.json'));
    const [row]=bindings.bindings, original=row.status_result.structuredContent;
    need(bindings.instance===ready.health.instance && ready.health.executable_sha256===audit.inputs.service.sha256,'service changed');
    const sdk=audit.compiled_sdk;
    for(const ref of Object.values(sdk))pinned(ref,false);
    pinned(audit.inputs.cli,false);
    const library=require(sdk['session.js'].path),runner=require(sdk['runner.js'].path);
    const image={path:audit.inputs.cli.path,sha256:audit.inputs.cli.sha256,version:ready.health.version};
    let exit, launches=0;
    const session=new library.AirkctlSession({clientVersion:image.version,maxQueued:1,launch(verified){
        need(++launches===1 && equal(verified,image),'session identity changed');
        const child=spawn(image.path,['--service-runtime-dir',ready.control_root,'session','--owner-watch','--label','AIRK CAL'],
            {stdio:'pipe',windowsHide:true,shell:false,env:runner.airkctlEnvironment()});
        child.stderr.pipe(process.stderr);
        exit=new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',code=>resolve(code));});
        return child;
    }},()=>{need(Date.now()<plan.start_ms+600000,'fixture deadline');});
    const events=[];
    async function invoke(tool,args){
        need(Date.now()+125000<plan.start_ms+600000,'original call headroom');
        const argv=['tools','call',tool,'--args',JSON.stringify(args),'--output','json'];
        const lo=Date.now(),result=await session.invoke(argv,{deadlineMs:125000}),hi=Date.now();
        need(result.exitCode===0 && result.class==='success' && session.hello.serviceInstance===bindings.instance,'native call refused');
        const envelope=JSON.parse(result.stdout), body=envelope.structuredContent||JSON.parse(envelope.content[0].text);
        events.push({tool,argv,result,lo,hi});
        return body;
    }
    try{
        session.bind(image);
        const grant=await acquire(session,row.window_scope_selection,bindings.instance,e=>events.push(e));
        const status=await invoke('status',{response_mode:'full',include_status:true});
        const scope=windowStatus(status,{scope_observation:original.scope_observation,publication:row.publication,
            window_selection:row.window_scope_selection,files:original.files,indexed_vectors:original.indexed_vectors},grant);
        const search=await invoke('search',plan.cases[label].args);
        need(equal(scope,search.scope_observation),'Search scope differs');
        await release(session,e=>events.push(e));
    }finally{
        session.dispose();
        if(exit)need(await exit===0,'original SDK session failed');
    }
    process.stdout.write(JSON.stringify(events));
}
main().catch(error=>{process.stderr.write(String(error.stack));process.exitCode=1;});
