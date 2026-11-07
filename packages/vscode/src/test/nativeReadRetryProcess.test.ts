// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import {NativeProcessError} from '../native';
import {readNative,capturedNativeText} from './nativeReadRetry';

const busy={isError:true,content:[{type:'text',text:JSON.stringify({error:{
    code:'direct_query_admission',reason:'busy',query_submitted:false,
    retryable:true,retry_after_ms:1,fresh_brokerage_required:false,
}})}]};

// Real child processes exercise execFile's exit-code/stdout contract on each host.
async function fixture(run:(directory:string,counter:string)=>Promise<void>):Promise<void>{
    const directory=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-read-retry-')));
    try{await run(directory,path.join(directory,'calls'));}
    finally{await fs.rm(directory,{recursive:true,force:true});}
}
function program(counter:string,body:string):string{
    return `const fs=require('node:fs');const file=${JSON.stringify(counter)};`+
        `const calls=fs.existsSync(file)?Number(fs.readFileSync(file,'utf8'))+1:1;fs.writeFileSync(file,String(calls));${body}`;
}

test('CLI reads repeat exit-one typed unsubmitted refusals and return the actual later reply',async()=>{
    await fixture(async(directory,counter)=>{
        const confirmed={content:[{type:'text',text:'{"files":1}'}]};
        const source=program(counter,`console.log(JSON.stringify(calls===1?${JSON.stringify(busy)}:${JSON.stringify(confirmed)}));process.exitCode=calls===1?1:0;`);
        assert.deepEqual(await readNative(process.execPath,['-e',source],directory),confirmed);
        assert.equal(await fs.readFile(counter,'utf8'),'2');
    });
});

test('CLI reads never replay permanent, submitted, malformed or unexpected-exit failures',async()=>{
    const submitted={isError:true,content:[{type:'text',text:JSON.stringify({error:{
        code:'direct_query_admission',reason:'busy',query_submitted:true,
        retryable:true,retry_after_ms:1,fresh_brokerage_required:false,
    }})}]};
    for(const [reply,code] of [[busy,2],[submitted,1],[{isError:true,content:[]},1],
        [{...busy,structuredContent:{hits:[]}},1]] as const){
        await fixture(async(directory,counter)=>{
            const source=program(counter,`console.log(${JSON.stringify(JSON.stringify(reply))});console.error('original process failure');process.exitCode=${code};`);
            await assert.rejects(readNative(process.execPath,['-e',source],directory),error=>{
                assert.ok(error instanceof NativeProcessError);
                assert.equal(error.code,code);assert.ok(error.stderr.includes('original process failure'));
                const failure=error as NativeProcessError&{nativeStdout:string;nativeStderr:string;nativeResponse:unknown};
                assert.equal(failure.nativeStdout,JSON.stringify(reply)+'\n');
                assert.equal(failure.nativeStderr,'original process failure\n');
                assert.deepEqual(failure.nativeResponse,reply);return true;
            });
            assert.equal(await fs.readFile(counter,'utf8'),'1');
        });
    }
    await fixture(async(directory,counter)=>{
        const source=program(counter,"console.log('not JSON');");
        await assert.rejects(readNative(process.execPath,['-e',source],directory),/no JSON envelope/);
        assert.equal(await fs.readFile(counter,'utf8'),'1');
    });
});

test('CLI reads admit no process after their observation deadline',async()=>{
    await fixture(async(directory,counter)=>{
        await assert.rejects(readNative(process.execPath,['-e',program(counter,'')],directory,performance.now()-1),/original observation budget/);
        await assert.rejects(fs.stat(counter),{code:'ENOENT'});
    });
});

test('one-shot captured native failure retains complete stdout without replay',async()=>{
    await fixture(async(directory,counter)=>{
        const stdout=JSON.stringify({isError:true,structuredContent:{error:{code:'fixture_failed',retryable:false}}})+'\n';
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


test('CLI scoped retries accept forward metadata clocks without rewriting launch arguments',async()=>{
    const selection={schema:1,context_key:'06'.repeat(32),namespace:'07'.repeat(16),expected_catalog_revision:7,
        grant_id:'grt_'+'03'.repeat(16),expected_grant_revision:1,index_id:'idx_'+'01'.repeat(16),project_ids:['prj_'+'02'.repeat(16)]};
    const confirmed={content:[{type:'text',text:'{"files":0}'}]};
    for(const code of ['scoped_status_prepare_timeout','scoped_connection_unavailable']){
        const body={error:{code,query_submitted:false,retryable:true,retry_after_ms:1,fresh_brokerage_required:true,
            ...(code==='scoped_connection_unavailable'?{native_control_response:{status:'failure',code:'project_capacity_busy'}}:{})},
            requested_scope:{confirmed:false,selection:{...selection,expected_catalog_revision:12}}};
        const setup={isError:true,content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body};
        await fixture(async(directory,counter)=>{
            const original=JSON.stringify(selection);
            const source=program(counter,`if(process.argv[process.argv.indexOf('--catalog-session')+1]!==${JSON.stringify(original)})throw new Error('original launch selector changed');`+
                `console.log(JSON.stringify(calls===1?${JSON.stringify(setup)}:${JSON.stringify(confirmed)}));process.exitCode=calls===1?1:0;`);
            const args=['-e',source,'--','--catalog-session',original];
            assert.deepEqual(await readNative(process.execPath,args,directory),confirmed);
            assert.equal(await fs.readFile(counter,'utf8'),'2');
        });
        for(const selected of [{...selection,expected_grant_revision:2},{...selection,expected_catalog_revision:13}]){
            await fixture(async(directory,counter)=>{
                const source=program(counter,`console.log(${JSON.stringify(JSON.stringify(setup))});process.exitCode=1;`);
                const args=['-e',source,'--','--catalog-session',JSON.stringify(selected)];
                await assert.rejects(readNative(process.execPath,args,directory),error=>error instanceof NativeProcessError&&error.code===1);
                assert.equal(await fs.readFile(counter,'utf8'),'1');
            });
        }
    }
});
