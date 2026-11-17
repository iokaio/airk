# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
B=Path(__file__).parent;W=B/'p10-worktree';O=B/'post-p14-node-failure-diagnostics-proposed-r1';O.mkdir(exist_ok=False);changes={}
p='packages/vscode/src/test/nativeServiceFixture.ts';a=(W/p).read_text(encoding='utf-8');s=a
s=s.replace("import {runText} from '../native';", "import {runText,NativeProcessError} from '../native';")
i=s.index('export class NativeServiceFixture')
s=s[:i]+'''export interface NativeTestFailure {error:unknown;context?:RecordValue}
/** Test diagnostics preserve the original error and actual captured native reply. */
export function nativeFailureDetail(error:unknown):RecordValue {
    if(error instanceof AggregateError)return {name:error.name,message:error.message,
        errors:Array.from(error.errors).slice(0,16).map(nativeFailureDetail),
        omitted_errors:Math.max(0,error.errors.length-16)};
    const value=error as {nativeStdout?:string;nativeStderr?:string;nativeResponse?:unknown;detail?:unknown}|null;
    return {name:error instanceof Error?error.name:typeof error,message:String(error).slice(0,8192),
        ...(error instanceof Error?{stack:error.stack?.slice(0,8192)}:{}),
        ...(error instanceof NativeProcessError?{code:error.code,signal:error.signal,killed:error.killed,
            timedOut:error.timedOut,elapsedMs:error.elapsedMs,timeoutMs:error.timeoutMs}:{}),
        ...(typeof value?.nativeStdout==='string'?{stdout:value.nativeStdout,stderr:value.nativeStderr,
            stdio_capture_limit_bytes:16*1024*1024,stdio_maybe_truncated:
                error instanceof NativeProcessError&&error.code==='ERR_CHILD_PROCESS_STDIO_MAXBUFFER'}:{}),
        ...(value?.nativeResponse!==undefined?{nativeResponse:value.nativeResponse}:{}),
        ...(value?.detail!==undefined?{detail:value.detail}:{})};
}

'''+s[i:]
s=s.replace('    private retained=false;', "    private retained=false;\n    private lastPhase:RecordValue={phase:'fixture_setup'};")
s=s.replace("        process.stderr.write(`[native fixture] ${phase}\\n`);", "        this.lastPhase={phase};\n        process.stderr.write(`[native fixture] ${phase}\\n`);")
s=s.replace("        process.stderr.write(`[native fixture] tool ${name}\\n`);", "        this.lastPhase={phase:'tool',tool:name};\n        process.stderr.write(`[native fixture] tool ${name}\\n`);")
s=s.replace('async retainFailure(error:unknown,sessions:readonly McpStdioSession[]=[]):Promise<void>{', 'async retainFailure(error:unknown,sessions:readonly McpStdioSession[]=[],context?:RecordValue):Promise<void>{')
s=s.replace("        const parent=process.env.AIRK_NATIVE_DIAGNOSTIC_DIR;\n        if(!parent)return;", "        const parent=process.env.AIRK_NATIVE_DIAGNOSTIC_DIR??path.join(this.owner,'test-failure-diagnostics');")
s=s.replace("                error:String(error).slice(0,8192),memory:", "                error:String(error).slice(0,8192),failure:nativeFailureDetail(error),phase:this.lastPhase,context,memory:")
i=s.index('    async dispose():Promise<void>{');s=s[:i]+'''    async dispose(failure?:NativeTestFailure,sessions:readonly McpStdioSession[]=[]):Promise<void>{
        const errors:unknown[]=failure?[failure.error]:[];
        let stopped=false;
        try{
            if(this.client){
                const health=record((await this.client.rpc('health','health')).observation),inspection=await this.run(['runtime','inspect','--install-dir',this.install]);
                const request={caller:randomBytes(16).toString('hex'),key:await this.client.key(),action:'stop',expected_instance:health.instance,runtime:record(inspection.selection).active,force:false};
                const receipt=await this.run(['runtime','lifecycle','--install-dir',this.install],180000,JSON.stringify(request));
                assert.equal(record(receipt.record).phase,'complete',JSON.stringify(receipt));
                stopped=true;
            }
        }catch(error){errors.push(error);}
        finally{
            // A failed health/inspect/Stop must still close its exact console
            // adapter. This is not evidence of native Stop or worker absence.
            try{this.client?.transport.dispose();}catch(error){errors.push(error);}
            this.client=undefined;
        }
        if(!errors.length&&!stopped){
            try{if(await fs.stat(this.install).then(()=>true,()=>false))
                await this.run(['runtime','uninstall','--install-dir',this.install],180000);
            }catch(error){errors.push(error);}
        }
        if(errors.length){
            const error=errors.length===1?errors[0]:new AggregateError(errors,'Native test and owned cleanup failed');
            await this.retainFailure(error,sessions,failure?.context);
            throw error;
        }
        // Physical fixture disposal follows only the qualified Stop/uninstall.
        if(this.retained){process.stderr.write(`[native fixture] stopped fixture retained: ${this.owner}\\n`);return;}
        const resolved=await fs.realpath(this.owner),temporary=await fs.realpath(process.env.AIRK_PRIVATE_CENSUS_BASE??os.tmpdir());
        assert.equal(path.dirname(resolved),temporary);assert.ok(path.basename(resolved).startsWith('airk-native-'));
        await fs.rm(resolved,{recursive:true,force:true,maxRetries:20,retryDelay:100});
    }
}
''';changes[p]=(a,s)
p='packages/vscode/src/test/nativeReadRetry.ts';a=(W/p).read_text(encoding='utf-8');s=a.replace("import {NativeProcessError} from '../native';", "import {NativeProcessError} from '../native';\nimport {requireNativeExecution} from '../nativeGate';")
s+='''
/** One-shot test read with runText's exact process policy and complete failure streams. */
export function capturedNativeText(command:string,args:readonly string[],cwd:string,timeout:number):Promise<string>{
    try{requireNativeExecution();}catch(error){return Promise.reject(error);}
    const started=performance.now();
    return new Promise((resolve,reject)=>{
        execFile(command,[...args],{cwd,timeout,maxBuffer:16*1024*1024,windowsHide:true,encoding:'utf8',
            env:{...process.env,AIRK_CLIENT:'vscode',AIRK_ERROR_FORMAT:'json'}},(error,stdout,stderr)=>{
            if(error){reject(Object.assign(new NativeProcessError(command,error,stderr,
                Math.round(performance.now()-started),timeout),{nativeStdout:stdout,nativeStderr:stderr}));}
            else resolve(stdout);
        });
    });
}
''';changes[p]=(a,s)
p='packages/vscode/src/test/native.test.ts';a=(W/p).read_text(encoding='utf-8');s=a.replace("import { NativeServiceFixture } from './nativeServiceFixture';", "import { NativeServiceFixture,NativeTestFailure,nativeFailureDetail } from './nativeServiceFixture';\nimport {capturedNativeText} from './nativeReadRetry';")
start=s.index("test('extension native clients index,");end=s.index("\ntest('native queue waits",start);part=s[start:end]
part=part.replace('    try{', "    let failure:NativeTestFailure|undefined;\n    let phase='native client assertions';\n    try{",1)
part=part.replace("        const usage=record(JSON.parse(await runText(a.command,[...launchArgs(a,'usage'),'report'],path.dirname(a.command),120000)));", "        phase='usage report';\n        const usage=record(JSON.parse(await capturedNativeText(a.command,[...launchArgs(a,'usage'),'report'],path.dirname(a.command),120000)));")
part=part.replace('    }finally{await fixture.dispose();}', '    }catch(error){failure={error,context:{phase}};}finally{await fixture.dispose(failure);}')
s=s[:start]+part+s[end:]
s+='''
test('native fixture cleanup closes transport and preserves primary plus health failure',async()=>{
    const directory=await fs.mkdtemp(path.join(os.tmpdir(),'airk-native-cleanup-unit-'));
    const primary=new Error('original body failure'),health=new Error('health unavailable');
    let disposed=0;
    const fixture=Object.create(NativeServiceFixture.prototype) as NativeServiceFixture;
    Object.assign(fixture,{owner:directory,install:path.join(directory,'not-created'),lastPhase:{phase:'unit'}});
    fixture.client={rpc:async()=>{throw health;},transport:{dispose:()=>{disposed++;}}} as unknown as NonNullable<NativeServiceFixture['client']>;
    try{
        await assert.rejects(fixture.dispose({error:primary}),error=>{
            assert.ok(error instanceof AggregateError);assert.deepEqual(error.errors,[primary,health]);return true;
        });
        assert.equal(disposed,1);assert.equal(fixture.client,undefined);
        assert.ok((await fs.stat(directory)).isDirectory());
        const children=await fs.readdir(path.join(directory,'test-failure-diagnostics'));
        const saved=JSON.parse(await fs.readFile(path.join(directory,'test-failure-diagnostics',children[0],'failure.json'),'utf8'));
        assert.equal(saved.failure.errors[0].message,String(primary));
        assert.equal(saved.failure.errors[1].message,String(health));
    }finally{await fs.rm(directory,{recursive:true,force:true});}
});
test('native failure detail retains exact process stdout and independent stderr',()=>{
    const stdout=JSON.stringify({isError:true,structuredContent:{error:{code:'actual_failure',retryable:false}}});
    const failure=Object.assign(new NativeProcessError('airk',Object.assign(new Error('failed'),{code:1}),
        'generic stderr',12,120000),{nativeStdout:stdout,nativeStderr:'exact stderr\\n'});
    const detail=nativeFailureDetail(failure);
    assert.equal(detail.stdout,stdout);assert.equal(detail.stderr,'exact stderr\\n');
    assert.equal(detail.code,1);assert.equal(detail.stdio_maybe_truncated,false);
});
''';changes[p]=(a,s)
p='packages/vscode/src/test/explorerAssistanceNative.test.ts';a=(W/p).read_text(encoding='utf-8');s=a.replace("import { NativeServiceFixture } from './nativeServiceFixture';", "import { NativeServiceFixture,NativeTestFailure } from './nativeServiceFixture';")
s=s.replace('    let host: AssistanceIpcHost | undefined, session: McpStdioSession | undefined;', "    let host: AssistanceIpcHost | undefined, session: McpStdioSession | undefined;\n    let failure:NativeTestFailure|undefined,lastRead:unknown;\n    let phase='assistance setup';")
s=s.replace("        await fixture.run(['runtime','lifecycle'", "        phase='explicit Stop';\n        await fixture.run(['runtime','lifecycle'",1)
s=s.replace('        session = (await McpStdioSession.open(', "        phase='open unified endpoint after Stop';\n        session = (await McpStdioSession.open(",1)
s=s.replace("            const result = await requestWithReadRetry(session!, name, args); return result.structuredContent ?? JSON.parse(result.content[0].text);", "            phase=`tool ${name}`;\n            const result = await requestWithReadRetry(session!, name, args,120000,reply=>{lastRead={name,reply};}); return result.structuredContent ?? JSON.parse(result.content[0].text);")
old='''    } finally {
        session?.close(); host?.dispose();
        try { await fs.promises.rm(storage, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 }); }
        finally { await fixture.dispose(); }
    }'''
new='''    } catch(error) { failure={error,context:{phase,lastRead}}; }
    finally {
        const errors:unknown[]=failure?[failure.error]:[];
        try{session?.close();}catch(error){errors.push(error);}
        try{host?.dispose();}catch(error){errors.push(error);}
        try{await fs.promises.rm(storage,{recursive:true,force:true,maxRetries:20,retryDelay:100});}
        catch(error){errors.push(error);}
        const error=errors.length>1?new AggregateError(errors,'Assistance body and local cleanup failed'):errors[0];
        await fixture.dispose(errors.length?{error,context:failure?.context}:undefined,session?[session]:[]);
    }'''
assert old in s;s=s.replace(old,new);changes[p]=(a,s)
p='packages/vscode/src/test/nativeReadRetryProcess.test.ts';a=(W/p).read_text(encoding='utf-8');s=a.replace("import {readNative} from './nativeReadRetry';", "import {readNative,capturedNativeText} from './nativeReadRetry';")
s+='''
test('one-shot captured native failure retains complete stdout without replay',async()=>{
    await fixture(async(directory,counter)=>{
        const stdout=JSON.stringify({isError:true,structuredContent:{error:{code:'fixture_failed',retryable:false}}})+'\\n';
        const source=program(counter,`process.stdout.write(${JSON.stringify(stdout)});process.stderr.write('exact stderr');process.exitCode=1;`);
        await assert.rejects(capturedNativeText(process.execPath,['-e',source],directory,5000),error=>{
            assert.ok(error instanceof NativeProcessError);
            assert.equal((error as NativeProcessError&{nativeStdout:string}).nativeStdout,stdout);
            assert.equal((error as NativeProcessError&{nativeStderr:string}).nativeStderr,'exact stderr');
            assert.equal(error.code,1);return true;
        });
        assert.equal(await fs.readFile(counter,'utf8'),'1');
    });
});
''';changes[p]=(a,s)
m={'status':'inert_external_test_diagnostics_and_cleanup_only_not_applied_not_run','files':[],'new_tests':3,'scope':'Preserve primary and cleanup failures; always close exact console transport; no physical fixture deletion absent qualified Stop/uninstall; preserve actual usage CLI stdout through same one-shot gate/args/deadline/cap, no replay; assistance retains last actual reply and stage. Native production/frozen pair untouched.'};patch=''
for p,(a,s) in changes.items():
 f=O/'source'/p;f.parent.mkdir(parents=True,exist_ok=True);f.write_text(s,encoding='utf-8',newline='\n');m['files'].append({'path':p,'base_sha256':hashlib.sha256((W/p).read_bytes()).hexdigest(),'proposed_sha256':hashlib.sha256(f.read_bytes()).hexdigest()});patch+=''.join(difflib.unified_diff(a.splitlines(True),s.splitlines(True),fromfile='a/'+p,tofile='b/'+p))
(O/'fix.patch').write_text(patch,encoding='utf-8',newline='\n');m['patch_sha256']=hashlib.sha256((O/'fix.patch').read_bytes()).hexdigest();(O/'manifest.json').write_text(json.dumps(m,indent=2)+'\n',encoding='utf-8');print(m['patch_sha256'])
