// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import { createHash } from 'node:crypto';
import { spawnBounded, type Spawner } from '../runner';
import { authenticodeAccepted, systemVerifyHost, windowsOwnerPrivate, type VerifyHost } from '../verify';
import { verifyAirkAttestation, decodeAttestationBundle, type ProvenanceExpectation } from './provenance';
import { SIGNING_IDENTITY } from '../signingIdentity';
import type { InstallerOffer, VerifiedInstaller } from './controller';
import { RELEASE_API, releaseAsset, selectInstaller, selectRelease, approvedRedirect, approvedAttestationBundleUrl, type OfficialRelease } from './release';

function installerEnvironment():NodeJS.ProcessEnv{
    const env:NodeJS.ProcessEnv={...process.env,LC_ALL:'C'};
    for(const name of Object.keys(env)){if(['BASH_ENV','ENV','CDPATH','SHELLOPTS','BASHOPTS'].includes(name)||name.startsWith('BASH_FUNC_')){delete env[name];}}
    return env;
}
const MAX_METADATA = 2 * 1024 * 1024;
const MAX_MANIFEST = 4 * 1024 * 1024;
async function officialFetch(initial: string, signal: AbortSignal, metadata = false, requester:typeof fetch=fetch): Promise<Response> {
    let url=initial;
    for (let redirects=0;redirects<=4;redirects++) {
        if (!approvedRedirect(initial,url)) {throw new Error('Unexpected release download origin or path.');}
        const response=await requester(url,{signal,redirect:'manual',headers:metadata?{Accept:'application/vnd.github+json','User-Agent':'AIRK-VSCode-bootstrap'}:{'User-Agent':'AIRK-VSCode-bootstrap'}});
        if (![301,302,303,307,308].includes(response.status)) {return response;}
        const next=response.headers.get('location');
        await response.body?.cancel();
        if (!next) {throw new Error('Release redirect has no location.');}
        url=new URL(next,url).toString();
    }
    throw new Error('Release redirect limit exceeded.');
}
async function bytes(url: string, maximum: number, signal: AbortSignal,requester:typeof fetch=fetch): Promise<Buffer> {
    const response = await officialFetch(url,AbortSignal.any([signal,AbortSignal.timeout(120000)]),true,requester);
    if (!response.ok || !response.body) { throw new Error(`Official release request failed (${response.status}).`); }
    const parts: Buffer[] = []; let used = 0;
    for await (const part of response.body as unknown as AsyncIterable<Uint8Array>) {
        used += part.length;
        if (used > maximum) { throw new Error('Release metadata exceeds its bound.'); }
        parts.push(Buffer.from(part));
    }
    return Buffer.concat(parts);
}
const sha = (value: Buffer) => createHash('sha256').update(value).digest('hex');
/** Standard OS Gatekeeper assessment, with no developer tool or consent UI. */
export async function assessMacInstaller(spawner:Spawner,file:string,directory:string):Promise<boolean>{
    const result=await spawner('/usr/sbin/spctl',['--assess','--type','open','--context','context:primary-signature','--verbose=2',file],
        {env:installerEnvironment(),cwd:directory,timeoutMs:60000,maxStdout:65536,maxStderr:65536});
    return result.exitCode===0&&!result.timedOut&&!result.overflow&&/^source=Notarized Developer ID\r?$/m.test(Buffer.concat([result.stdout,result.stderr]).toString('utf8'));
}
/** Injection ports for deterministic adapter failures; production uses only defaults. */
export interface InstallerSystemPorts {
    readonly requester?:typeof fetch;
    readonly platform?:NodeJS.Platform;
    readonly verifier?:VerifyHost;
    readonly spawner?:Spawner;
    readonly stagingRoot?:string;
    readonly verifyProvenance?:(value:unknown,expected:ProvenanceExpectation)=>void;
    readonly openFile?:typeof fs.open;
}
export class SystemInstaller {
    private readonly verifier:VerifyHost;
    private readonly requester:typeof fetch;
    private readonly spawner:Spawner;
    private readonly offers = new Map<string, { offer: InstallerOffer; manifest: Record<string, unknown> }>();
    private readonly staged = new Map<string, { offer: InstallerOffer; file: string; directory: string; manifest: Record<string, unknown> }>();
    constructor(private readonly version: string, private readonly target: string, private readonly log: (line: string) => void, private readonly stage: (message:string)=>void = ()=>{}, private readonly ports:InstallerSystemPorts={}) {
        this.verifier=ports.verifier??systemVerifyHost(false);this.requester=ports.requester??fetch;this.spawner=ports.spawner??spawnBounded;
    }
    async discover(signal: AbortSignal): Promise<InstallerOffer> {
        let release: OfficialRelease | undefined;
        for (let page=1;page<=3;page++) {
            const rows:unknown=JSON.parse((await bytes(`${RELEASE_API}?per_page=20&page=${page}`,MAX_METADATA,signal,this.requester)).toString('utf8'));
            if (!Array.isArray(rows) || rows.length>20) {throw new Error('Official release page exceeds its bound.');}
            if (rows.some(row=>row?.tag_name===`v${this.version}`)) {release=selectRelease(rows,this.version);break;}
            if (rows.length<20) {break;}
        }
        if (!release) {throw new Error('Compatible AIRK release was not found in the bounded catalog. Use manual installation.');}
        const releaseFile = releaseAsset(release,'release.json',MAX_METADATA);
        const raw = await bytes(releaseFile.url,MAX_METADATA,signal,this.requester);
        if (raw.length !== releaseFile.bytes || releaseFile.digest && sha(raw) !== releaseFile.digest) { throw new Error('Official release metadata digest/size differs.'); }
        const record = JSON.parse(raw.toString('utf8')) as Record<string, unknown>;
        const offer = selectInstaller(release,record,this.target);
        const manifestName = `airk-${offer.version}-${offer.target}-installer.manifest.json`;
        const manifestAsset = releaseAsset(release,manifestName,MAX_MANIFEST);
        const manifestRaw = await bytes(manifestAsset.url,MAX_MANIFEST,signal,this.requester);
        const hashes = record.assets as Record<string,string>;
        if (sha(manifestRaw) !== hashes[manifestName] || manifestRaw.length !== manifestAsset.bytes || manifestAsset.digest && sha(manifestRaw) !== manifestAsset.digest) { throw new Error('Installer manifest digest differs.'); }
        const manifest = JSON.parse(manifestRaw.toString('utf8')) as Record<string, unknown>;
        if (manifest.schema !== 'airk-standalone-installer/1' || manifest.commit !== record.commit || manifest.version !== offer.version || manifest.target !== offer.target || manifest.artifact !== offer.name || manifest.artifact_sha256 !== offer.sha256) { throw new Error('Installer manifest identity differs.'); }
        this.stage('Verifying official build provenance');
        await this.provenance({name:offer.name,sha256:offer.sha256,commit:String(record.commit),runId:String(record.run_id),attempt:String(record.attempt)},signal);
        this.offers.set(offer.url,{offer:{...offer,headlessArguments:['--silent']},manifest});
        return offer;
    }
    private async provenance(expected:ProvenanceExpectation,signal:AbortSignal):Promise<void>{
        const url=`https://api.github.com/repos/iokaio/airk/attestations/sha256:${expected.sha256}?per_page=5&predicate_type=https%3A%2F%2Fslsa.dev%2Fprovenance%2Fv1`;
        const response=JSON.parse((await bytes(url,MAX_METADATA,signal,this.requester)).toString('utf8')) as {attestations?:unknown};
        if(!Array.isArray(response.attestations)||response.attestations.length<1||response.attestations.length>5){throw new Error('No bounded official build attestations are available. Use manual instructions.');}
        for(const row of response.attestations){
            if(!row||typeof row!=='object'||row.repository_id!==1418150478){continue;}
            let bundle:unknown=row.bundle;
            if(!bundle){
                if(typeof row.bundle_url!=='string'||!approvedAttestationBundleUrl(row.bundle_url)){continue;}
                bundle=decodeAttestationBundle(await bytes(row.bundle_url,MAX_METADATA,signal,this.requester));
            }
            try{(this.ports.verifyProvenance??verifyAirkAttestation)(bundle,expected);return;}catch{ /* A different attestation never qualifies this installer. */ }
        }
        throw new Error('Installer provenance does not match the official AIRK release workflow, source and run. Use manual instructions.');
    }
    private async command(file: string, args: readonly string[], directory: string, timeoutMs = 600000, admitted = false): Promise<number> {
        const result = await this.spawner(file,args,{ env:installerEnvironment(), cwd:directory, timeoutMs,maxStdout:1<<20,maxStderr:1<<20,preserveProcessOnLimit:admitted });
        this.log(result.stdout.toString('utf8')); this.log(result.stderr.toString('utf8'));
        if (result.timedOut || result.overflow || result.exitCode === null) { throw new Error(`Installer process outcome is unknown. Recovery directory: ${directory}`); }
        return result.exitCode;
    }
    async downloadAndVerify(offer: InstallerOffer, signal: AbortSignal): Promise<VerifiedInstaller> {
        const known = this.offers.get(offer.url);
        if (!known || JSON.stringify(known.offer) !== JSON.stringify(offer)) { throw new Error('Installer offer changed.'); }
        const directory = await fs.mkdtemp(path.join(this.ports.stagingRoot??os.tmpdir(),'airk-installer-'));
        await fs.chmod(directory,0o700);
        if ((this.ports.platform??process.platform) === 'win32') {
            const system = path.join(process.env.SystemRoot ?? 'C:\\Windows','System32');
            const observation = await this.verifier.windows!([directory],path.join(system,'WindowsPowerShell','v1.0','powershell.exe'));
            if (await this.command(path.join(system,'icacls.exe'),[directory,'/inheritance:r','/grant:r',`*${observation.user}:(OI)(CI)F`,'*S-1-5-18:(OI)(CI)F','*S-1-5-32-544:(OI)(CI)F','/q'],directory,30000) !== 0) { throw new Error('Cannot restrict installer recovery directory.'); }
        }
        const file = path.join(directory,offer.name);
        const response = await officialFetch(offer.url,AbortSignal.any([signal,AbortSignal.timeout(600000)]),false,this.requester);
        if (!response.ok || !response.body) { throw new Error('Installer download failed.'); }
        let handle:Awaited<ReturnType<typeof fs.open>>;
        try{handle=await (this.ports.openFile??fs.open)(file,'wx',0o600);}catch(error){await response.body.cancel();throw error;}
        const created=await handle.stat();
        const digest = createHash('sha256'); let used = 0,lastPercent=-1;
        let downloaded=false;
        try {
            for await (const part of response.body as unknown as AsyncIterable<Uint8Array>) {
                if(signal.aborted){throw new Error('Installer download cancelled.');}
                used += part.length;
                if (used > offer.bytes) { throw new Error('Installer exceeds its declared size.'); }
                digest.update(part); await handle.writeFile(part);
                const percent=Math.floor(used*100/offer.bytes);
                if(percent!==lastPercent){this.stage(`Downloading installer: ${percent}%`);lastPercent=percent;}
            }
            if(signal.aborted){throw new Error('Installer download cancelled.');}
            await handle.sync();
            if (used !== offer.bytes || digest.digest('hex') !== offer.sha256) {throw new Error('Installer size or digest differs.');}
            downloaded=true;
        } finally {
            await handle.close();
            if (!downloaded) {
                const current=await fs.lstat(file).catch(()=>undefined);
                if (current?.isFile() && !current.isSymbolicLink() && current.dev===created.dev && current.ino===created.ino) {await fs.unlink(file);}
            }
        }
        this.stage('Verifying installer digest and publisher');
        this.staged.set(directory,{offer:known.offer,file,directory,manifest:known.manifest});
        await this.verify(directory);
        const payload=known.manifest.payload;
        const prefix=offer.target.endsWith('apple-darwin')?'payload/AIRK.app/Contents/MacOS/':'payload/';
        const servicePath=prefix+(offer.target.endsWith('apple-darwin')?'airk':offer.target.endsWith('windows-msvc')?'service/airk.exe':'service/airk');
        const cliPath=prefix+(offer.target.endsWith('apple-darwin')?'airkctl':offer.target.endsWith('windows-msvc')?'bin/airkctl.exe':'bin/airkctl');
        if (!payload || typeof payload!=='object' || Array.isArray(payload)) {throw new Error('Installer payload identity is absent.');}
        const serviceSha256=Reflect.get(payload,servicePath),cliSha256=Reflect.get(payload,cliPath);
        if (typeof serviceSha256!=='string' || !/^[a-f0-9]{64}$/.test(serviceSha256) || typeof cliSha256!=='string' || !/^[a-f0-9]{64}$/.test(cliSha256)) {throw new Error('Installer pair image identity is absent.');}
        return {offer,token:directory,images:{serviceSha256,cliSha256}};
    }
    private async verify(token: string): Promise<void> {
        const item = this.staged.get(token);
        if (!item) { throw new Error('Unrecognized private installer.'); }
        const facts = await fs.lstat(item.file);
        const parent = await fs.lstat(item.directory);
        if (!parent.isDirectory() || parent.isSymbolicLink() || (this.ports.platform??process.platform) !== 'win32' && ((parent.mode & 0o077) !== 0 || parent.uid !== process.getuid?.())) { throw new Error('Installer directory privacy changed.'); }
        if (!facts.isFile() || facts.isSymbolicLink() || facts.nlink !== 1 || facts.size !== item.offer.bytes || await this.verifier.sha256(item.file) !== item.offer.sha256) { throw new Error('Private installer bytes changed.'); }
        if (item.offer.signature === 'authenticode') {
            const observation = await this.verifier.windows!([item.directory,item.file],item.file);
            if (observation.acl.some(acl => windowsOwnerPrivate(acl,observation.user)) || authenticodeAccepted(observation.signature)) { throw new Error('Installer publisher, identity EKU or private ACL verification failed.'); }
        } else if (item.offer.signature === 'developer_id') {
            if(!await this.verifier.codesign!(item.file,SIGNING_IDENTITY.macos.team_id)||!await assessMacInstaller(this.spawner,item.file,item.directory)){throw new Error('Installer Developer ID or notarization verification failed.');}
        }
    }
    async execute(installer: VerifiedInstaller, signal: AbortSignal, admitted:()=>Promise<void> = async()=>{}): Promise<{exitCode:number;receipt:string}> {
        if (signal.aborted) { throw new Error('Installation cancelled before launch.'); }
        this.stage('Verifying installer');
        await this.verify(installer.token);
        const item = this.staged.get(installer.token)!;
        this.log(`AIRK installer recovery directory: ${item.directory}`);
        let exitCode: number;
        if ((this.ports.platform??process.platform) === 'win32') {
            if(signal.aborted){throw new Error('Installation cancelled before native launch.');}
            await admitted();
            exitCode = await this.command(item.file,['--silent',`/LOG=${path.join(item.directory,'setup.log')}`],item.directory,600000,true);
        } else if ((this.ports.platform??process.platform) === 'darwin') {
            const mount = path.join(item.directory,'mounted'); await fs.mkdir(mount,0o700);
            if (await this.command('/usr/bin/hdiutil',['attach',item.file,'-mountpoint',mount,'-readonly','-nobrowse','-noautoopen'],item.directory,60000) !== 0) { throw new Error('Cannot attach the verified installer image.'); }
            let settled=false,nativeLaunched=false;
            try {
                if(signal.aborted){throw new Error('Installation cancelled before native launch.');}
                await admitted();
                nativeLaunched=true;
                exitCode = await this.command('/bin/bash',[path.join(mount,'Install AIRK.command'),'--silent'],mount,600000,true);settled=true; }
            finally {
                if (settled||!nativeLaunched) {await this.command('/usr/bin/hdiutil',['detach',mount],item.directory,60000);}
                else {this.log(`Installer outcome is unknown; readonly mount retained: ${mount}`);}
            }
        } else {
            const result = await this.spawner('/usr/bin/tar',['-tzf',item.file],{env:installerEnvironment(),cwd:item.directory,timeoutMs:30000,maxStdout:1<<20,maxStderr:1<<20});
            const root = item.offer.name.slice(0,-7);
            const names = result.stdout.toString('utf8').trim().split('\n');
            if (result.exitCode !== 0 || result.timedOut || result.overflow || names.some(name => !name.startsWith(root+'/') || name.includes('\\') || name.split('/').includes('..') || name.startsWith('/'))) { throw new Error('Unsafe installer archive paths.'); }
            // Extract only the published regular-file roster; no archive links are accepted.
            const types = await this.spawner('/usr/bin/tar',['-tvzf',item.file],{env:installerEnvironment(),cwd:item.directory,timeoutMs:30000,maxStdout:1<<20,maxStderr:1<<20});
            if (types.exitCode !== 0 || types.overflow || types.timedOut || types.stdout.toString('utf8').trim().split('\n').some(line => !/^[d-]/.test(line))) { throw new Error('Installer archive contains links or special entries.'); }
            if (await this.command('/usr/bin/tar',['-xzf',item.file,'--no-same-owner','--no-same-permissions','-C',item.directory],item.directory,60000) !== 0) { throw new Error('Cannot extract installer.'); }
            if(signal.aborted){throw new Error('Installation cancelled before native launch.');}
            await admitted();
            exitCode = await this.command('/bin/sh',[path.join(item.directory,root,'install.sh'),'--silent'],item.directory,600000,true);
        }
        return {exitCode,receipt:item.directory};
    }
}
