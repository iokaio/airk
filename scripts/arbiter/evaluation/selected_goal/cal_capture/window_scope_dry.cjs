// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), crypto = require('node:crypto'), path = require('node:path');
const {spawn} = require('node:child_process');
const {isDeepStrictEqual:equal} = require('node:util');
const {acquire,release,windowStatus} = require('./window_scope.cjs');
const ROOT = path.resolve(__dirname,'../../../../..');
const E = path.join(ROOT,'artifacts/v190/final-plan/exec-20261116T052942Z');
const DRY = 'airk.private.cal1024-window-scope-dry/1';
function need(ok,msg){if(!ok)throw Error(msg);}
function file(p, expected) {
    const b=fs.readFileSync(p);
    need(crypto.createHash('sha256').update(b).digest('hex')===expected,'dry input identity differs');return JSON.parse(b);
}
async function main() {
    const [requestPath,expected]=process.argv.slice(2);
    const req=file(requestPath,expected);
    need(req.schema===DRY && path.dirname(requestPath).startsWith(E+path.sep) && process.env.AIRK_CAL_DRY_TICKET===requestPath,'checked owner required');
    const audit=file(path.join(E,'cal100-focus1024-launch-r1/cal100-launch-audit.json'),'87731282ecd121eaf08d5074d1bc421c5753bc65f7126f9578b31c6f9303d661');
    const ready=file(req.ready.path,req.ready.sha256), source=file(req.bindings.path,req.bindings.sha256).bindings[0];
    const cli=audit.inputs.cli, instance=ready.health.instance, sdk=audit.compiled_sdk;
    for(const p of [cli,...Object.values(sdk)]){
        need(crypto.createHash('sha256').update(fs.readFileSync(p.path)).digest('hex')===p.sha256,'dry runtime changed');
    }
    const {AirkctlSession}=require(sdk['session.js'].path);
    const runner=require(sdk['runner.js'].path);
    let child, exited, launches=0,closing=false;
    const retain=data=>process.stdout.write(JSON.stringify(data)+'\n');
    const session=new AirkctlSession({clientVersion:ready.health.version,maxQueued:1,launch(image) {
        need(++launches===1 && image.sha256===cli.sha256,'window dry session restarted');
        child=spawn(cli.path,['--service-runtime-dir',ready.control_root,'session','--owner-watch','--label','AIRK CAL W4 dry'],
            {windowsHide:true,shell:false,stdio:'pipe',env:runner.airkctlEnvironment()});
        exited=new Promise(resolve=>child.once('exit',(code,signal)=>resolve({code,signal})));
        child.stderr.on('data',bytes=>retain({kind:'session_stderr',bytes:bytes.toString('base64')}));return child;
    }});
    session.onEvent(event=>{retain({kind:'session_event',event});if(!closing && ['resync','closing','session_lost'].includes(event.kind))throw Error('dry custody lost');});
    session.bind({path:cli.path,sha256:cli.sha256,version:ready.health.version});
    async function status(named){
        const argv=['tools','call','status',...(named?['--index',source.index_id]:[]),'--args',JSON.stringify({response_mode:'full',include_status:true}),'--output','json'];
        const result=await session.invoke(argv,{deadlineMs:20000});
        retain({kind:'dry_status',named,result});
        need(result.exitCode===0 && result.class==='success' && session.hello?.serviceInstance===instance,'dry native status refused');
        return JSON.parse(result.stdout).structuredContent;
    }
    try{
        const original=await status(true), bound=source.status_result.structuredContent;
        // The native worker fingerprint includes the restarted worker incarnation.
        const {fingerprint: priorFingerprint,...priorScope}=bound.scope_observation;
        const {fingerprint: freshFingerprint,...freshScope}=original.scope_observation;
        need(/^[a-f0-9]{64}$/.test(priorFingerprint) && /^[a-f0-9]{64}$/.test(freshFingerprint) &&
            equal(freshScope,priorScope) && original.publication.token===source.publication.token &&
            original.files===753 && original.semantic_readiness.state==='ready','original settled source changed');
        const binding={scope_observation:original.scope_observation,publication:source.publication,
            window_selection:source.window_scope_selection,files:original.files,indexed_vectors:original.indexed_vectors};
        for(let i=0;i<2;i++){
            const g=await acquire(session,source.window_scope_selection,instance,retain), observed=await status(false);
            windowStatus(observed,binding,g);
            need(observed.files===original.files && observed.indexed_vectors===original.indexed_vectors &&
                equal(observed.semantic_readiness,original.semantic_readiness),'window source/semantic coverage differs');
            await release(session,retain);
        }
        retain({kind:'dry_complete',instance,files:original.files,publication:source.publication,scoring_calls:0});
    }finally{
        closing=true;session.dispose();
        if(exited){const end=await exited;need(end.code===0 && end.signal===null,'dry SDK child did not close normally');}
    }
}
main().catch(e=>{console.error(e.message);process.exitCode=1;});
