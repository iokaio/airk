// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import {NativeProcessError} from '../native';
import {readNative} from './nativeReadRetry';

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
    for(const [reply,code] of [[busy,2],[submitted,1],[{isError:true,content:[]},1]] as const){
        await fixture(async(directory,counter)=>{
            const source=program(counter,`console.log(${JSON.stringify(JSON.stringify(reply))});console.error('original process failure');process.exitCode=${code};`);
            await assert.rejects(readNative(process.execPath,['-e',source],directory),error=>
                error instanceof NativeProcessError&&error.code===code&&error.stderr.includes('original process failure'));
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
