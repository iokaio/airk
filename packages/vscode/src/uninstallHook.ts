// SPDX-License-Identifier: Apache-2.0
/** Hand off before VS Code's five-second hook deadline; cleanup survives extension removal. */
import * as fs from 'node:fs/promises';
import { createReadStream, constants } from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { nativeRunner } from './serviceBootstrap';
import { record } from './serviceClient';
import { uninstallTargets } from './extensionUninstall';

async function regular(file:string,max:number):Promise<void>{
    const info=await fs.lstat(file);
    if(!info.isFile()||info.isSymbolicLink()||info.nlink!==1||info.size>max)throw new Error('Unsafe uninstall snapshot input: '+file);
}
async function json(file:string,max:number):Promise<{bytes:Buffer;value:Record<string,unknown>}>{
    await regular(file,max);const bytes=await fs.readFile(file);
    if(bytes.length>max)throw new Error('Uninstall metadata exceeds its bound');
    return {bytes,value:record(JSON.parse(bytes.toString('utf8')))};
}
const digest=(bytes:Buffer)=>createHash('sha256').update(bytes).digest('hex');
const engineName=()=>process.platform==='win32'?'airk.exe':'airk';

/** Copy only the trusted distribution's main engine and standalone hook, never models or user data. */
export async function prepareUninstallWorker(extensionRoot:string,script:string):Promise<string>{
    const pkg=(await json(path.join(extensionRoot,'package.json'),1024*1024)).value;
    const native=path.join(extensionRoot,'native'),wrapper=await json(path.join(native,'manifest.json'),65536);
    const prefix=process.platform==='darwin'?'AIRK.app/Contents/MacOS/':'',name=prefix+engineName();
    const inventory=await json(path.join(native,'runtime','airk-install.json'),512*1024);
    if(wrapper.value.schema!==2||wrapper.value.version!==pkg.version||wrapper.value.vsix_target!==`${process.platform}-${process.arch}`
        ||wrapper.value.runtime_dir!=='runtime'||wrapper.value.install_manifest_sha256!==digest(inventory.bytes)
        ||inventory.value.schema!==1||inventory.value.version!==pkg.version
        ||record(wrapper.value.payload)[name]!==record(inventory.value.files)[name]
        ||typeof record(inventory.value.files)[name]!=='string')throw new Error('Uninstall distribution identity mismatch');
    const image=path.join(native,'runtime',name);
    await regular(image,2*1024*1024*1024);await regular(script,4*1024*1024);
    const job=await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()),'airk-uninstall-'));
    await fs.chmod(job,0o700);
    // A standalone engine needs no helper or inference library for lifecycle commands.
    await fs.copyFile(image,path.join(job,engineName()),constants.COPYFILE_EXCL);
    await fs.copyFile(script,path.join(job,'worker.js'),constants.COPYFILE_EXCL);
    await fs.writeFile(path.join(job,'approval.json'),JSON.stringify({schema:1,version:pkg.version,
        sha256:record(inventory.value.files)[name]}),{flag:'wx',mode:0o600});
    return job;
}
export async function verifyUninstallWorker(job:string):Promise<string>{
    const approval=(await json(path.join(job,'approval.json'),4096)).value,image=path.join(job,engineName());
    if(approval.schema!==1||typeof approval.sha256!=='string'||!/^[a-f0-9]{64}$/.test(approval.sha256))throw new Error('Invalid uninstall worker approval');
    await regular(image,2*1024*1024*1024);
    const hash=createHash('sha256');let bytes=0;
    for await(const block of createReadStream(image)){bytes+=block.length;if(bytes>2*1024*1024*1024)throw new Error('Uninstall engine grew');hash.update(block);}
    if(hash.digest('hex')!==approval.sha256)throw new Error('Uninstall engine differs from its approved distribution');
    return image;
}
export async function startUninstallWorker(job:string):Promise<void>{
    const log=await fs.open(path.join(job,'cleanup.log'),'ax',0o600);
    try{
        const child=spawn(process.execPath,[path.join(job,'worker.js'),'--airk-uninstall-worker'],{
            detached:true,windowsHide:true,stdio:['ignore',log.fd,log.fd],env:{...process.env,ELECTRON_RUN_AS_NODE:'1'},
        });
        await new Promise<void>((resolve,reject)=>{child.once('error',reject);child.once('spawn',()=>resolve());});
        child.unref();
    }finally{await log.close();}
}
async function cleanup(job:string):Promise<void>{
    let failure:string|undefined;
    try{
        const main=await verifyUninstallWorker(job);
        await uninstallTargets(main,nativeRunner,message=>process.stdout.write(message+'\n'));
    }catch(error){failure=String(error);process.stderr.write(failure+'\n');}
    await fs.writeFile(path.join(job,'result.json'),JSON.stringify({schema:1,finishedAt:new Date().toISOString(),
        state:failure?'needs_attention':'complete',...(failure?{failure}:{})}),{mode:0o600});
    // Keep the finite diagnostic result/log; retire this job's copied executable and script.
    for(const name of [engineName(),'approval.json','worker.js'])await fs.unlink(path.join(job,name)).catch(error=>process.stderr.write(`Uninstall worker cleanup: ${String(error)}\n`));
    if(failure)process.exitCode=1;
}
async function main():Promise<void>{
    if(process.argv.includes('--airk-uninstall-worker'))return cleanup(__dirname);
    const job=await prepareUninstallWorker(path.resolve(__dirname,'..'),__filename);
    await startUninstallWorker(job);
    process.stdout.write(`AIRK uninstall cleanup started. Result and log: ${job}\n`);
}
if(require.main===module)void main().catch(error=>{process.stderr.write(String(error)+'\n');process.exitCode=1;});
