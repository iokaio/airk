// SPDX-License-Identifier: Apache-2.0
/** Real native client fixture. All writable paths and service owners belong to this test. */
import {strict as assert} from 'node:assert';
import {createHash} from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import {LaunchSpec,launchArgs} from '../model';
import {runText} from '../native';
import {boundedReply,readNative} from './nativeReadRetry';
import {NativeControlTransport,RecordValue,ServiceClient,record,JobKind,ServiceError} from '../serviceClient';

export function nativePayloadDirectory(binary:string):string {
    const directory=path.dirname(path.resolve(binary)),contents=path.dirname(directory),app=path.dirname(contents);
    return path.basename(directory)==='MacOS'&&path.basename(contents)==='Contents'&&path.basename(app)==='AIRK.app'?path.dirname(app):directory;
}

/** Only bounded diagnostic facts are logged; error messages can include private paths. */
export function statusObservationSummary(status:RecordValue):RecordValue {
    const object=(value:unknown):RecordValue=>value!==null&&typeof value==='object'&&!Array.isArray(value)?value as RecordValue:{};
    const runtime=object(status.runtime),work=object(status.work),service=object(runtime.service),worker=object(work.worker);
    const code=object(status.index_error).code;
    return {index_state:['available','unavailable'].includes(String(status.index_state))?status.index_state:'invalid',
        code:['index_status_unavailable','index_worker_unavailable'].includes(String(code))?code:'unknown',
        service_responsive:typeof service.responsive==='boolean'?service.responsive:null,service_accepting:typeof service.accepting==='boolean'?service.accepting:null,
        service_observation_available:runtime.service_error===null&&Object.keys(service).length>0,
        automatic_start_blocked:typeof worker.automatic_start_blocked==='boolean'?worker.automatic_start_blocked:null,
        metadata_samples_complete:Array.isArray(work.failed_observations)&&work.failed_observations.length===0};
}

/** Unavailable observations can conceal a permanent cause; they are not typed retry hints. */
export function publishedStatusPending(status:RecordValue,collection:string,project:string):boolean {
    const detail=JSON.stringify(statusObservationSummary(status));
    if(status.index_state==='available'){
        const publication=record(status.publication);
        assert.ok(status.collection===collection&&publication.collection===collection
            &&typeof publication.token==='string'&&publication.token.length>0
            &&Number.isSafeInteger(status.files)&&Number(status.files)>=0,detail);
        return false;
    }
    assert.ok(status.index_state==='unavailable'&&status.publication===null&&status.files===null&&status.chunks===null,detail);
    const observation=record(status.index_error),runtime=record(status.runtime),work=record(status.work);
    const worker=record(work.worker),service=record(runtime.service);
    assert.ok(typeof observation.message==='string',detail);
    if(observation.code==='index_status_unavailable')assert.ok(status.collection===collection,detail);
    else{
        assert.ok(observation.code==='index_worker_unavailable'&&status.project===project,detail);
        assert.ok(work.serving_worker_unavailable===true&&record(runtime.index_worker).process===null,detail);
    }
    assert.ok(runtime.service_error===null&&service.responsive===true&&service.accepting===true
        &&work.basis==='instance_bound_service_metadata'&&worker.automatic_start_blocked===false
        &&Array.isArray(work.failed_observations)&&work.failed_observations.length===0,detail);
    if(worker.identity!==null){const identity=record(worker.identity);
        assert.ok(identity.project===project&&typeof service.instance==='string'&&identity.service===service.instance,detail);}
    return true;
}

export class NativeServiceFixture {
    client?:ServiceClient;
    control='';launcher='';
    private constructor(readonly binary:string,readonly owner:string,readonly install:string){}
    static async create(binary:string):Promise<NativeServiceFixture>{
        const owner=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-native-')));
        const fixture=new NativeServiceFixture(path.resolve(binary),owner,path.join(owner,'i'));
        try{
            const source=nativePayloadDirectory(fixture.binary),bytes=await fs.readFile(path.join(source,'airk-install.json'));
            const manifest=JSON.parse(bytes.toString());
            const extension=JSON.parse(await fs.readFile(path.resolve(__dirname,'../../package.json'),'utf8'));
            assert.equal(manifest.version,extension.version);
            await fixture.run(['runtime','managed','--source',source,'--install-dir',fixture.install,
                '--state-dir',path.join(owner,'s'),'--approved-sha256',createHash('sha256').update(bytes).digest('hex'),
                '--verification-basis','organization-digest','--lifecycle','user-on-demand'],180000);
            const prepared=await fixture.run(['runtime','prepare-client','--install-dir',fixture.install],180000);
            const inspected=await fixture.run(['runtime','inspect','--install-dir',fixture.install]);
            assert.equal(inspected.launcher,prepared.launcher);assert.equal(record(inspected.managed_policy).lifecycle,'user_on_demand');
            fixture.control=String(inspected.control_dir);fixture.launcher=String(prepared.launcher);
            fixture.client=new ServiceClient(new NativeControlTransport(fixture.binary,['service','console','--runtime-dir',fixture.control],60000));
            return fixture;
        }catch(error){
            try{await fixture.dispose();}catch(cleanup){throw new AggregateError([error,cleanup],'Native fixture setup and cleanup failed');}
            throw error;
        }
    }
    async run(args:string[],timeout=120000,input?:string):Promise<RecordValue>{
        const phase=args[0]==='runtime'?`runtime ${args[1]}`:args.includes('prepare-registration')?'prepare-registration':'prepare-storage';
        process.stderr.write(`[native fixture] ${phase}\n`);
        try{return record(JSON.parse(await runText(this.binary,args,path.dirname(this.binary),timeout,undefined,undefined,input)));}
        catch(error){process.stderr.write(`[native fixture] ${phase} failed: ${String(error)}\n`);throw error;}
    }
    async requireResidentProjects(count:number):Promise<void>{
        assert.ok(Number.isInteger(count)&&count>=1&&count<=4);
        const before=record((await this.client!.rpc('resources','resources')).observation);
        if(Number(before.resident_limit)>=count)return;
        // These tests keep multiple project transports live simultaneously.
        // The default 2 GiB budget on an 8 GiB host intentionally admits one.
        // Request only the missing 512 MiB resident reservations through the
        // public policy API on this disposable service; keep admission enabled.
        const memory=Number(before.memory_limit_bytes)+(count-Number(before.resident_limit))*512*1024*1024;
        assert.ok(memory<=Number(before.detected_memory_bytes),'Native test needs enough detected memory for its concurrent projects');
        const configuration={...record(before.configuration),memory_bytes:memory};
        const after=record((await this.client!.rpc({configure_resources:{configuration}},'resources')).observation);
        assert.ok(Number(after.resident_limit)>=count,'Requested test residency was not admitted');
        assert.ok(Number(after.memory_limit_bytes)<=Number(after.detected_memory_bytes));
    }
    async register(name:string,source?:string):Promise<LaunchSpec>{
        const root=source??path.join(this.owner,name);await fs.mkdir(root,{recursive:true});
        const data=path.join(this.owner,'data');await fs.mkdir(data,{recursive:true});
        const registration=await this.run(['--root',root,'--data-dir',data,'--collection',name,'service','prepare-registration']);
        const project=String(registration.binding);
        const reserved=await this.client!.rpc({reserve_project:{project:registration}},'registered');
        assert.deepEqual(record(reserved.project).registration,registration);
        await this.run(['service','prepare-storage','--runtime-dir',this.control,'--project',project]);
        return {command:this.launcher,root:String(registration.root),dataDir:String(registration.data_root),collection:name,name,
            service:{installDir:this.install,project}};
    }
    async tool(spec:LaunchSpec,name:string,args:RecordValue={},deadline?:number):Promise<RecordValue>{
        process.stderr.write(`[native fixture] tool ${name}\n`);
        const envelope=record(await readNative(spec.command,[...launchArgs(spec,'tools'),'call',name,'--args',
            JSON.stringify({response_mode:'full',...args})],path.dirname(spec.command),deadline));
        assert.notEqual(envelope.isError,true,`${name} failed: ${boundedReply(envelope)}`);assert.ok(Array.isArray(envelope.content),boundedReply(envelope));
        return record(JSON.parse(String(record(envelope.content[0]).text)));
    }
    async publishedStatus(spec:LaunchSpec,deadline=performance.now()+120000):Promise<RecordValue>{
        let unavailable:RecordValue|undefined;
        while(performance.now()<deadline){
            const status=await this.tool(spec,'airk_diagnostics',{},deadline);
            assert.ok(performance.now()<deadline,'Published status observation deadline exhausted');
            if(!publishedStatusPending(status,spec.collection,spec.service!.project))return status;
            const observation=statusObservationSummary(status);
            if(unavailable===undefined)process.stderr.write(`[native fixture] waiting for published status: ${JSON.stringify(observation)}\n`);
            unavailable=observation;
            await new Promise(resolve=>setTimeout(resolve,25));
        }
        throw new Error(`Published status remained unavailable: ${JSON.stringify(unavailable)}`);
    }
    async settings(spec:LaunchSpec,view:string):Promise<RecordValue>{
        return record(JSON.parse(await runText(spec.command,[...launchArgs(spec,'settings'),view],path.dirname(spec.command),120000)));
    }
    async embedding():Promise<RecordValue>{
        const deadline=Date.now()+15000;
        while(true){
            try{return await this.client!.rpc('embedding_status','embedding_status');}
            catch(error){if(!(error instanceof ServiceError)||error.code!=='embedding_observation_busy'||Date.now()>=deadline)throw error;}
            await new Promise(resolve=>setTimeout(resolve,50));
        }
    }
    async ensureEmbedding():Promise<RecordValue>{
        if(!(await this.embedding()).available){
            try{await this.client!.rpc('start_embedding','embedding_starting');}
            catch(error){if(!(error instanceof ServiceError)||error.code!=='embedding_already_running')throw error;}
        }
        const deadline=Date.now()+90000;
        while(Date.now()<deadline){const observed=await this.embedding();if(observed.available)return observed;await new Promise(resolve=>setTimeout(resolve,50));}
        throw new Error('Owned helper did not become available');
    }
    async waitJob(id:string):Promise<RecordValue>{
        const deadline=Date.now()+180000;
        while(Date.now()<deadline){
            const job=record((await this.client!.rpc({job:{id}},'job')).job);
            if(['succeeded','failed','canceled'].includes(String(job.state)))return job;
            await new Promise(resolve=>setTimeout(resolve,25));
        }
        throw new Error('Owned native job did not finish');
    }
    async job(spec:LaunchSpec,kind:JobKind='index'):Promise<RecordValue>{
        process.stderr.write(`[native fixture] job ${kind}\n`);
        const intent=await this.client!.prepareJob(spec.service!.project,kind);
        const accepted=await this.client!.submitJob(intent),job=await this.waitJob(String(record(accepted.job).id));
        assert.equal(job.state,'succeeded',JSON.stringify(job));return job;
    }
    async configure(spec:LaunchSpec,draft:RecordValue):Promise<RecordValue>{
        process.stderr.write(`[native fixture] configure ${String(draft.operation)}\n`);
        const accepted=record(JSON.parse(await runText(spec.command,[...launchArgs(spec,'configure'),'--stdin'],
            path.dirname(spec.command),180000,undefined,undefined,JSON.stringify(draft))));
        const job=await this.waitJob(String(record(accepted.job).id));assert.equal(job.state,'succeeded',JSON.stringify(job));return job;
    }
    async ready(spec:LaunchSpec,files:number):Promise<RecordValue>{
        const deadline=performance.now()+180000;
        while(performance.now()<deadline){
            const status=await this.publishedStatus(spec,deadline),readiness=record(status.semantic_readiness);
            assert.ok(['disabled','pending','partial','ready','failed'].includes(String(readiness.state)),'Invalid semantic state');
            assert.notEqual(readiness.state,'failed','Semantic work explicitly failed');
            if(readiness.state==='ready'&&readiness.completed_artifacts===files)return status;
            await new Promise(resolve=>setTimeout(resolve,50));
        }
        throw new Error('Owned semantic work did not finish');
    }
    async repair(spec:LaunchSpec,operation:'verify'|'rebuild'|'auto'='verify'):Promise<RecordValue>{
        process.stderr.write(`[native fixture] repair ${operation}\n`);
        const created=await this.client!.submitMutation(await this.client!.preparePlan([spec.service!.project],operation));
        const id=String(record(created.plan).id),deadline=Date.now()+180000;
        const wait=async(phase:string)=>{
            while(Date.now()<deadline){
                const plan=record((await this.client!.rpc({plan:{id}},'plan')).plan);
                if(plan.phase===phase)return plan;
                await new Promise(resolve=>setTimeout(resolve,25));
            }
            throw new Error('Owned repair plan did not finish');
        };
        const reviewed=await wait('ready');assert.match(String(reviewed.preview_digest),/^[a-f0-9]{64}$/);
        await this.client!.rpc({plan_execute:{id,preview:reviewed.preview_digest,allow_fallback:false}},'plan');
        const completed=await wait('finished');assert.equal(completed.outcome,'succeeded',JSON.stringify(completed));return completed;
    }
    async dispose():Promise<void>{
        this.client?.transport.dispose();this.client=undefined;
        // Native uninstall authenticates and drains only this fixture's approved
        // user-on-demand owner. A failed drain preserves the directory for diagnosis.
        if(await fs.stat(this.install).then(()=>true,()=>false))await this.run(['runtime','uninstall','--install-dir',this.install],180000);
        const resolved=await fs.realpath(this.owner),temporary=await fs.realpath(os.tmpdir());
        assert.equal(path.dirname(resolved),temporary);assert.ok(path.basename(resolved).startsWith('airk-native-'));
        await fs.rm(resolved,{recursive:true,force:true,maxRetries:20,retryDelay:100});
    }
}
