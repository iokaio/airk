// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { prepareUninstallWorker, verifyUninstallWorker } from '../uninstallHook';

test('uninstall snapshots only approved main bytes and survives deletion of the extension directory',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-hook-snapshot-'));
    let job:string|undefined;
    try{
        const runtime=path.join(root,'native','runtime');await fs.mkdir(runtime,{recursive:true});
        const name=(process.platform==='darwin'?'AIRK.app/Contents/MacOS/':'')+(process.platform==='win32'?'airk.exe':'airk');
        const image=path.join(runtime,name),body=Buffer.from('Approved, inert engine fixture.');
        await fs.mkdir(path.dirname(image),{recursive:true});await fs.writeFile(image,body);
        const hash=(bytes:Buffer)=>createHash('sha256').update(bytes).digest('hex');
        const inventory=Buffer.from(JSON.stringify({schema:1,version:'1.6.1',files:{[name]:hash(body)}}));
        await fs.writeFile(path.join(runtime,'airk-install.json'),inventory);
        await fs.writeFile(path.join(root,'package.json'),JSON.stringify({version:'1.6.1'}));
        await fs.writeFile(path.join(root,'native','manifest.json'),JSON.stringify({schema:2,version:'1.6.1',vsix_target:`${process.platform}-${process.arch}`,
            runtime_dir:'runtime',install_manifest_sha256:hash(inventory),payload:{[name]:hash(body)}}));
        const script=path.join(root,'hook.js');await fs.writeFile(script,'// Self-contained inert fixture.');
        job=await prepareUninstallWorker(root,script);
        const copied=await verifyUninstallWorker(job);
        assert.deepEqual((await fs.readdir(job)).sort(),['approval.json',path.basename(image),'worker.js'].sort());
        await fs.rm(root,{recursive:true,force:true});
        assert.equal(await verifyUninstallWorker(job),copied);
        await fs.writeFile(copied,'Modified snapshot');
        await assert.rejects(verifyUninstallWorker(job),/differs from its approved distribution/);
    }finally{await fs.rm(root,{recursive:true,force:true});if(job)await fs.rm(job,{recursive:true,force:true});}
});

test('a detached cleanup worker outlives both its hook process and the five-second VS Code deadline',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-hook-handoff-')),job=path.join(root,'job');await fs.mkdir(job);
    try{
        const done=path.join(job,'done');
        await fs.writeFile(path.join(job,'worker.js'),`setTimeout(()=>require('node:fs').writeFileSync(${JSON.stringify(done)},'complete'),5500);`);
        const parent=path.join(root,'hook.js'),module=require.resolve('../uninstallHook');
        await fs.writeFile(parent,`require(${JSON.stringify(module)}).startUninstallWorker(${JSON.stringify(job)}).catch(e=>{console.error(e);process.exitCode=1;});`);
        const started=Date.now();
        await promisify(execFile)(process.execPath,[parent],{timeout:4500,env:{...process.env,ELECTRON_RUN_AS_NODE:'1'}});
        assert.ok(Date.now()-started<5000,'the hook must finish within the editor deadline');
        await fs.unlink(parent);
        const deadline=Date.now()+10000;
        while(!await fs.stat(done).then(()=>true,()=>false)&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,50));
        assert.equal(await fs.readFile(done,'utf8'),'complete');
        assert.ok(Date.now()-started>=5500,'cleanup continues after the hook is gone');
    }finally{await fs.rm(root,{recursive:true,force:true});}
});
