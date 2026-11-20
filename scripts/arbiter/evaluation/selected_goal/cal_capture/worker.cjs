// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {spawn, execFile, execFileSync} = require('node:child_process');
const {promisify, isDeepStrictEqual:equal} = require('node:util');
const {capturePhase} = require('./phase.cjs');
const exec = promisify(execFile);
const ROOT = path.resolve(__dirname,'../../../../..');
const E = path.join(ROOT,'artifacts/v190/final-plan/exec-20261116T052942Z');
const AUDIT = '87731282ecd121eaf08d5074d1bc421c5753bc65f7126f9578b31c6f9303d661';
const sha = raw => crypto.createHash('sha256').update(raw).digest('hex');
function need(value,message) { if (!value) throw Error(message); }
function inside(parent,filename) {
    const relative = path.relative(parent,filename);
    return relative && !relative.startsWith('..' + path.sep) && relative !== '..' && !path.isAbsolute(relative);
}
function ordinary(filename,max) {
    need(path.isAbsolute(filename) && fs.realpathSync.native(filename).toLowerCase() === filename.toLowerCase(),
        'noncanonical input');
    const st = fs.lstatSync(filename);
    need(st.isFile() && st.nlink === 1 && st.size <= max,'invalid bounded ordinary file');
    return fs.readFileSync(filename);
}
function pinned(ref,raw=false) {
    const bytes=ordinary(ref.path,ref.bytes);
    need(bytes.length===ref.bytes && sha(bytes)===ref.sha256,'input identity differs');
    return raw ? bytes : JSON.parse(bytes);
}
function log(value) { fs.writeSync(1,JSON.stringify(value)+'\n'); }

async function run(request, phase, ticket, originalSeconds) {
    const start=performance.now();
    need(request.schema==='airk.private.cal100-live-fixture/1' && Number.isFinite(originalSeconds) &&
        originalSeconds>0 && originalSeconds<=1800, 'invalid original phase time');
    const raw=ordinary(path.join(E,'cal100-focus1024-launch-r1','cal100-launch-audit.json'),32<<10);
    need(sha(raw)===AUDIT,'launch-input audit differs');
    const audit=JSON.parse(raw), scopes=pinned(audit.inputs.scopes), census=pinned(audit.census);
    const work=request.fixture_root;
    need(inside(E,work) && fs.realpathSync.native(work).toLowerCase()===work.toLowerCase(),'foreign fixture');
    const checked=(ref,leaf)=>{
        need(path.dirname(ref.path)===work && (!leaf || path.basename(ref.path)===leaf),'foreign fixture document');
        return pinned(ref);
    };
    const ready=checked(request.ready,'ready.json'), bindings=checked(request.bindings), answer=checked(request.answer,'context-request.json');
    need(ready.schema==='airk.private-arbiter-fixture/1' && ready.evaluation===true && ready.release_qualified===false &&
        ready.health.executable_sha256===audit.inputs.service.sha256 && ready.inputs_sha256===audit.inputs.inputs.sha256 &&
        bindings.schema==='airk.private-arbiter-index-bindings/1' && bindings.evaluation===true && bindings.release_qualified===false &&
        bindings.instance===ready.health.instance && bindings.namespace===ready.health.catalog_clock.namespace &&
        bindings.control_root===ready.control_root && bindings.scopes_sha256===audit.inputs.scopes.sha256 &&
        bindings.bindings.length===1,'unbound native fixture or scope binding');
    const row=bindings.bindings[0];
    need(scopes.source_roots.length===1 && fs.realpathSync.native(row.source_root)===fs.realpathSync.native(scopes.source_roots[0]),
        'native source root differs');
    const admitted=path.join(work,'context-cal-admissions');
    need(fs.statSync(admitted).isDirectory() && fs.realpathSync.native(admitted).toLowerCase()===admitted.toLowerCase() &&
        !fs.existsSync(path.join(work,'closed.json')),'original observer fixture unavailable');
    const binding={index_id:row.index_id,instance:ready.health.instance,publication:row.publication,
        scope_observation:row.status_result.structuredContent.scope_observation,
        source_root_ids:row.source_roots.map(r=>r.id),pipeline:audit.pipeline,
        focus_manifest:audit.profiles['arbiter-focus-qwen3-reranker-06b'],
        context_manifest:audit.profiles['arbiter-context-qwen3-17b']};
    const sdk=audit.compiled_sdk;
    for(const ref of Object.values(sdk)) pinned(ref,true);
    pinned(audit.inputs.cli,true);
    const library=require(sdk['session.js'].path), runner=require(sdk['runner.js'].path);
    let child,exit,launches=0,closed=false,poisoned=false,session,originalState;
    const stop=()=>{poisoned=true; if(child && !closed) child.kill();};
    function check(ms) {
        need(!poisoned && performance.now()-start+ms < originalSeconds*1000 &&
            Date.now()+ms < ticket.deadline_ms && !fs.existsSync(path.join(work,'closed.json')) &&
            !fs.existsSync(path.join(work,'composed-observer.closed.json')),
            'original phase or fixture no longer available');
    }
    const image={path:audit.inputs.cli.path,sha256:audit.inputs.cli.sha256,version:ready.health.version};
    const launch=verified=>{
        check(0);
        need(++launches===1 && equal(verified,image),'session restarted or changed image');
        child=spawn(image.path,['--service-runtime-dir',ready.control_root,'session','--owner-watch','--label','AIRK CAL'],
            {stdio:'pipe',windowsHide:true,shell:false,env:runner.airkctlEnvironment()});
        exit=new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',(code,signal)=>resolve({code,signal}));});
        exit.catch(()=>{poisoned=true;});
        const reader=new library.FrameReader();
        child.stdout.on('data',bytes=>{
            try {
                for(const raw of reader.push(bytes)){
                    const frame=library.decodeFrame(JSON.parse(raw.toString('utf8')));
                    if(frame.kind==='reply' && (frame.outcome.code==='session_busy' ||
                        (frame.outcome.status==='hello' && frame.outcome.service_instance!==binding.instance))) stop();
                }
            } catch { stop(); }
        });
        child.stderr.on('data',bytes=>log({kind:'session_stderr',bytes:bytes.toString('base64')}));
        return child;
    };
    session=new library.AirkctlSession({clientVersion:image.version,launch,maxQueued:1},()=>check(0));
    session.onEvent(event=>{log({kind:'session_event',event});if(!closed && ['resync','closing','session_lost'].includes(event.kind)) stop();});
    async function stages(){
        check(20000);
        const common=['--output','json','--service-runtime-dir',ready.control_root];
        const args=[...common,'arbiter','status-legacy'];
        const {stdout}=await exec(image.path,args,{windowsHide:true,timeout:20000,maxBuffer:4<<20});
        const status=JSON.parse(stdout);
        for(const [name,manifest] of [['focus',binding.focus_manifest],['context',binding.context_manifest]]){
            need(status[name]?.availability==='ready' && status[name].selected_profile===manifest &&
                status[name].active_profile===manifest,'both frozen stages must be ready');
        }
        log({kind:'stage_status',status});
        return status.configuration;
    }
    function names(){return fs.readdirSync(admitted).filter(n=>/^record-\d{4}\.json$/.test(n)).sort();}
    const owner={session,checkRemaining:check,
        async beginPhase(){originalState=await stages();},
        async observationCursor(){return names();},
        async observationsAfter(before){
            const after=names();need(before.every(n=>after.includes(n)),'native record disappeared');
            return after.filter(n=>!before.includes(n)).map(name=>{
                const bytes=ordinary(path.join(admitted,name),32<<20);
                log({kind:'native_observation',name,sha256:sha(bytes)});return JSON.parse(bytes);
            });
        },
        async retainExchange(argv,result){log({kind:'native_exchange',argv,result});},
        async completeQuestion(evidence){log({kind:'cal_question',evidence});},
        async closePhase(){need(equal(originalState,await stages()),'stage configuration drift');}
    };
    try {
        session.bind(image);
        await capturePhase(owner,binding,{questions:scopes.questions,ids:census.question_ids,answer},phase);
    } finally {
        closed=true;session.dispose();
        if(exit){
            let timer;
            try {
                const result=await Promise.race([exit,new Promise((_,reject)=>{
                    timer=setTimeout(()=>reject(Error('session child did not settle')),10000);
                })]);
                need(result.code===0 && result.signal===null,'abnormal SDK child closure');
            } finally {clearTimeout(timer);}
        }
    }
    check(0);
    need(launches===1 && !poisoned,'SDK session custody failed');
    log({kind:'phase_complete',phase,ids:census.question_ids.slice(phase*8,(phase+1)*8),
        instance:binding.instance,publication:binding.publication});
}

if(require.main===module){
    const [file,digest,n,seconds]=process.argv.slice(2);
    try {
        need(process.env.AIRK_CAL100_LIVE_REQUEST === file && path.isAbsolute(process.env.AIRK_CAL100_PYTHON || ''),
            'use the owned CAL controller');
        execFileSync(process.env.AIRK_CAL100_PYTHON, ['-B', '-S', path.join(__dirname, 'controller.py'),
            '--verify-worker-job', String(process.pid)], {timeout:10000, windowsHide:true, stdio:'pipe'});
        const raw=ordinary(file,1<<20);
        need(inside(E,file) && sha(raw)===digest,'foreign live request');
        const request=JSON.parse(raw);
        const ticket=pinned(request.start_receipt);
        need(ticket.schema==='airk.private.cal100-fixture-start/1' && ticket.fixture_root===request.fixture_root &&
            ticket.input_audit_sha256===AUDIT && Number.isSafeInteger(ticket.start_ms) &&
            ticket.deadline_ms-ticket.start_ms===10800000 && ticket.start_ms<=Date.now(),
            'original fixture lifetime unavailable');
        run(request,Number(n),ticket,Number(seconds)).catch(e=>{console.error(e.message);process.exitCode=1;});
    } catch(e){console.error(e.message);process.exitCode=1;}
}
