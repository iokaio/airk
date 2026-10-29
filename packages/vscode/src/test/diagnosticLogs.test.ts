// SPDX-License-Identifier: Apache-2.0
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import type { BootstrapObservation } from '../serviceBootstrap';
import { recordDiagnosticFailure } from '../diagnosticLogs';
import { DiagnosticCaptures } from '../diagnosticCaptures';

test('client diagnostics retain bounded UTF-8 cause chains without requiring service RPC',async()=>{
    const observation={bundle:{main:path.resolve('verified','airk')},inspection:{control_dir:path.resolve('control')}} as unknown as BootstrapObservation;
    const cause=new Error('underlying file permission failure');
    await recordDiagnosticFailure(observation,'raw_logs',new Error('Open failed',{cause}),async(command,args,timeout,input)=>{
        assert.equal(command,observation.bundle.main);assert.equal(timeout,5000);assert.ok(args.includes('record-error'));
        const value=JSON.parse(input!);assert.match(value.detail.chain,/underlying file permission failure/);assert.equal(value.detail.truncated,false);
        return {recorded:true};
    });
    await recordDiagnosticFailure(observation,'mcp_check',new Error('🙂'.repeat(10000)),async(_command,_args,_timeout,input)=>{
        const value=JSON.parse(input!);assert.ok(Buffer.byteLength(value.detail.chain)<=8192);assert.equal(value.detail.truncated,true);return {recorded:true};
    });
    await assert.rejects(recordDiagnosticFailure(undefined,'mcp_check',cause),/No verified runtime/);
});

test('diagnostic captures are bounded, released individually and preserve unknown files',async context=>{
    const root=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-captures-')));
    context.after(()=>fs.rm(root,{recursive:true,force:true}));
    const captures=new DiagnosticCaptures(root);const files=await Promise.all([captures.allocate(),captures.allocate(),captures.allocate(),captures.allocate()]);
    for(const file of files)await fs.writeFile(file,'retained log');
    await assert.rejects(captures.allocate(),/Close an earlier/);
    const foreign=path.join(root,'keep.txt');await fs.writeFile(foreign,'user file');await captures.release(foreign);
    assert.equal(await fs.readFile(foreign,'utf8'),'user file');
    await captures.release(files[0]);await assert.rejects(fs.stat(files[0]));
    const replacement=await captures.allocate();assert.notEqual(replacement,files[0]);
    await captures.release(replacement);for(const file of files.slice(1))await captures.release(file);
    captures.dispose();
});
