// SPDX-License-Identifier: Apache-2.0
'use strict';
// Private control only. All I2 framing/cancellation belongs to the actual SDK.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),readline=require('node:readline');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
let readNativePolicy,readNativeDelivery;
const sessions=new Map(),jobs=new Map();
let packet,nativePolicy,nativeDelivery,helper,real;
const {performance}=require('node:perf_hooks');
function readPacket(filename,expected){
    if(!/^[a-f0-9]{64}$/.test(expected||''))throw Error('externally pinned controller packet required');
    const st=fs.lstatSync(filename);if(!st.isFile()||st.isSymbolicLink()||st.size>16777216)throw Error('bounded ordinary packet required');
    const raw=fs.readFileSync(filename);if(raw.length>16777216||sha(raw)!==expected)throw Error('controller packet drift');
    return JSON.parse(raw);
}
function verify(){for(const [p,h]of Object.entries(packet.pins)){const st=fs.lstatSync(p);if(!st.isFile()||st.isSymbolicLink()||st.size>16777216)throw Error('controller code/input pin too large or aliased');if(sha(fs.readFileSync(p))!==h)throw Error('input drift: '+p);}}
function initialize(filename,expected){
    packet=readPacket(filename,expected);
    if(packet.adapter!=='airk.maintained.composed-service/1')throw Error('maintained packet epoch required');
    for(const p of [packet.resource_policy_input.path,packet.native_delivery_input.path,require.resolve('./native_policy.cjs'),require.resolve('./native_delivery.cjs'),__filename])if(!packet.pins[p])throw Error('unsealed current policy/controller input: '+p);
    if(packet.session_helper!==path.join(__dirname,'native_eval_session.cjs'))throw Error('maintained sibling session helper required');
    for(const p of [packet.session_helper,packet.session_library,...packet.client_dependencies])if(!packet.pins[p])throw Error('unsealed actual SDK module: '+p);
    for(const name of ['runner.js','adminChunks.js','arbiterBudget.js'])if(!packet.client_dependencies.some(p=>path.basename(p)===name))throw Error('missing actual SDK dependency '+name);
    verify();
    ({readNativePolicy}=require('./native_policy.cjs'));
    ({readNativeDelivery}=require('./native_delivery.cjs'));
    nativePolicy=readNativePolicy(packet.resource_policy_input);
    nativeDelivery=readNativeDelivery(packet.native_delivery_input,nativePolicy.policy);
    if(nativePolicy.policy.schema!=='airk.arbiter.resource-policy/2'||nativePolicy.policy.deadline_ms!==120000||nativePolicy.policy.shared_input_tokens!==8192||nativePolicy.policy.focus_pair_input_tokens!==512||nativeDelivery.deadlineMs!==125000)throw Error('current native policy/delivery required');
    if(typeof packet.whole_phase_seconds!=='number'||!Number.isFinite(packet.whole_phase_seconds)||packet.whole_phase_seconds<=0||packet.whole_phase_seconds>1800)throw Error('original whole-phase allowance required');
    helper=require(packet.session_helper);real=require(packet.session_library);verify();
}
function errorObservation(error){return {message:error.message,code:error.code,sent:error.sent??null,outcome_unknown:error.outcomeUnknown??null,retryable:error.retryable??null,result:error.result??null,retry:false};}
function waitBudget(deadline,now){const left=Math.floor(deadline-now);if(left<1)throw Error('original event deadline exhausted');return left;}
function watchJournal(request){return new Promise((resolve,reject)=>{
    const ceiling=request.warm_observation===true?Math.floor(packet.whole_phase_seconds*1000):nativePolicy.policy.deadline_ms;
    if(!Number.isInteger(request.timeout_ms)||request.timeout_ms<1||request.timeout_ms>ceiling)throw Error('invalid original event deadline');
    const target=path.resolve(request.path),parent=path.dirname(target),deadline=performance.now()+request.timeout_ms;
    let watcher,timer,settled=false;
    const finish=(error,value)=>{if(settled)return;if(!error){try{waitBudget(deadline,performance.now());}catch(expired){error=expired;}}settled=true;watcher?.close();clearTimeout(timer);error?reject(error):resolve(value);};
    const inspect=()=>{try{
        waitBudget(deadline,performance.now());
        if(!fs.existsSync(target))return;
        if(fs.statSync(target).size>4194304)throw Error('bounded observer journal exceeded');
        const bytes=fs.readFileSync(target),lines=bytes.toString('utf8').split('\n');lines.pop();
        const events=lines.filter(Boolean).map(x=>JSON.parse(x));
        const matches=events.filter(e=>Object.entries(request.match).every(([k,v])=>JSON.stringify(e[k])===JSON.stringify(v)));
        if(matches.length){const event=matches.find(e=>(e.sequence??0)>(request.after_sequence??-1));if(event)finish(null,{event,journal_sha256:sha(bytes),observed_unix_ms:Date.now(),events});}
    }catch(error){finish(error);}};
    watcher=fs.watch(parent,inspect);timer=setTimeout(()=>finish(Error('observed-event deadline expired')),waitBudget(deadline,performance.now()));inspect();
});}
async function action(r){
    verify();
    if(r.kind==='open'){
        if(sessions.size>=2)throw Error('reviewed two-session controller bound exceeded');
        if(sessions.has(r.id))throw Error('session reuse');
        const holder={signal:null};
        // This forwards to the actual SDK, never simulates an invocation or frame.
        class ActualAbortSession extends real.AirkctlSession {invoke(argv,options){return super.invoke(argv,{...options,signal:holder.signal});}}
        if(!r.config.resource_policy_input||r.config.resource_policy_input.path!==packet.resource_policy_input.path||r.config.resource_policy_input.sha256!==packet.resource_policy_input.sha256)throw Error('current session/controller policy differs');
        const client=helper.createPinnedSession(r.config,{library:{...real,AirkctlSession:ActualAbortSession}});
        sessions.set(r.id,{client,holder,busy:false});return await client.initialize();
    }
    if(r.kind==='start'){
        if(!Number.isInteger(r.deadline_ms)||r.deadline_ms<1||r.deadline_ms>nativeDelivery.deadlineMs)throw Error('original invoke transport budget differs');
        if(jobs.has(r.id))throw Error('request replay');
        const s=sessions.get(r.session);if(!s)throw Error('unknown session');
        if(s.busy)throw Error('overlapping requests require independent actual sessions');s.busy=true;
        const abort=new AbortController();s.holder.signal=abort.signal;
        const started=Date.now();const promise=s.client.invoke({id:r.id,tool:r.tool,args_file:r.args_file,deadline_ms:r.deadline_ms})
            .then(value=>({outcome:'returned',value,started_unix_ms:started,completed_unix_ms:Date.now()}))
            .catch(error=>({outcome:'refused',error:errorObservation(error),started_unix_ms:started,completed_unix_ms:Date.now()})).finally(()=>{s.busy=false;});
        jobs.set(r.id,{abort,promise,awaited:false});return {submitted:true,id:r.id,started_unix_ms:started,dispatch_proved:false};
    }
    if(r.kind==='await'){const j=jobs.get(r.id);if(!j||j.awaited)throw Error('unknown/already observed original request');const result=await j.promise;j.awaited=true;return result;}
    if(r.kind==='abort'){const j=jobs.get(r.id);if(!j)throw Error('unknown original request');j.abort.abort();return {abort_signalled:true,id:r.id,unix_ms:Date.now(),completion_proved:false};}
    if(r.kind==='close'){const s=sessions.get(r.id);if(!s)throw Error('unknown session');const result=await s.client.close();sessions.delete(r.id);return result;}
    if(r.kind==='watch')return await watchJournal(r);
    if(r.kind==='finish'){if(sessions.size||[...jobs.values()].some(j=>!j.awaited))throw Error('unclosed sessions/unobserved requests');return {closed:true};}
    throw Error('unknown private action');
}
async function main(){const input=readline.createInterface({input:process.stdin,crlfDelay:Infinity});for await(const line of input){
    const r=JSON.parse(line);try{const value=await action(r);process.stdout.write(JSON.stringify({id:r.control_id,ok:true,value})+'\n');}
    catch(error){process.stdout.write(JSON.stringify({id:r.control_id,ok:false,error:error.message})+'\n');process.exitCode=1;break;}
    if(r.kind==='finish')break;
}}
if(require.main===module){initialize(process.argv[2],process.argv[3]);main().catch(e=>{process.stderr.write(String(e)+'\n');process.exitCode=1;});}
module.exports={readPacket,errorObservation,waitBudget};
