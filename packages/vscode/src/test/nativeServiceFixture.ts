// SPDX-License-Identifier: Apache-2.0
/** Real native client fixture. All writable paths and service owners belong to this test. */
import {strict as assert} from 'node:assert';
import {createHash} from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import {LaunchSpec,launchArgs} from '../model';
import {runText} from '../native';
import {readNative} from './nativeReadRetry';
import {NativeControlTransport,RecordValue,ServiceClient,record,JobKind,ServiceError} from '../serviceClient';

export function nativePayloadDirectory(binary:string):string {
    const directory=path.dirname(path.resolve(binary)),contents=path.dirname(directory),app=path.dirname(contents);
    return path.basename(directory)==='MacOS'&&path.basename(contents)==='Contents'&&path.basename(app)==='AIRK.app'?path.dirname(app):directory;
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
    async tool(spec:LaunchSpec,name:string,args:RecordValue={}):Promise<RecordValue>{
        process.stderr.write(`[native fixture] tool ${name}\n`);
        const envelope=record(await readNative(spec.command,[...launchArgs(spec,'tools'),'call',name,'--args',
            JSON.stringify({response_mode:'full',...args})],path.dirname(spec.command)));
        assert.notEqual(envelope.isError,true);assert.ok(Array.isArray(envelope.content));
        return record(JSON.parse(String(record(envelope.content[0]).text)));
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
        const deadline=Date.now()+180000;
        while(Date.now()<deadline){
            const status=await this.tool(spec,'airk_status'),readiness=record(status.semantic_readiness);
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
