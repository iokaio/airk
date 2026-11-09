// SPDX-License-Identifier: Apache-2.0
/** Private per-request journal. Memento remains a preference store, never the
 * authority for a request after its bounded legacy import has completed. */
import * as fs from 'node:fs';
import * as path from 'node:path';
import {createHash,randomUUID} from 'node:crypto';
import {isDeepStrictEqual} from 'node:util';
import {restrictWindowsDirectory} from './privateDirectory';
import {ServiceError} from './serviceClient';
import {IntentStorage,SavedIntent,INTENT_PREFIX,INTENT_LIMIT,INTENT_MAX_TOTAL_BYTES,validateStoredIntent,maximumRecordBytes,chargedRecordBytes} from './serviceIntents';
import {acceptedIntentSlots,selectAcceptedIntentEvidence,mergeAcceptedIntentEvidence,withoutAcceptedIntentEvidence,acceptedIntentEvidenceDominates,
    MAX_ACCEPTED_INTENT_SLOTS,MAX_UNRESERVED_ACCEPTED_INTENT_SLOTS,MIN_RESERVED_ACCEPTED_INTENT_BYTES,type AcceptedIntentEvidence} from './intentAcceptedEvidence';

const MAX_FILES=1024,MAX_FILE_BYTES=INTENT_MAX_TOTAL_BYTES+65536;
const ORDINARY_RECORD_BYTES=65536;
export const PRIVATE_INTENT_CAPTURE_FILES=INTENT_LIMIT*(2+MAX_UNRESERVED_ACCEPTED_INTENT_SLOTS)
    +Math.floor(INTENT_MAX_TOTAL_BYTES/MIN_RESERVED_ACCEPTED_INTENT_BYTES)*MAX_ACCEPTED_INTENT_SLOTS;
export const PRIVATE_INTENT_CAPTURE_BYTES=256*1024;
const RETIREMENT_FILES=MAX_ACCEPTED_INTENT_SLOTS+2,RETIREMENT_BYTES=32*1024;
/** Logical admission remains64 requests/2MiB effective SavedIntent values.
 * Physical reads also cover immutable initial snapshots, finite accepted
 * anchors and current overlays:6+32+12+2=52MiB, plus1MiB metadata. These are
 * conservative format ceilings, not added caller payload/history capacity. */
export const PRIVATE_INTENT_PHYSICAL_READ_BYTES=(INTENT_LIMIT*ORDINARY_RECORD_BYTES+INTENT_MAX_TOTAL_BYTES)
    +MAX_ACCEPTED_INTENT_SLOTS*INTENT_MAX_TOTAL_BYTES
    +MAX_UNRESERVED_ACCEPTED_INTENT_SLOTS*INTENT_LIMIT*ORDINARY_RECORD_BYTES+INTENT_MAX_TOTAL_BYTES+4*PRIVATE_INTENT_CAPTURE_BYTES;
const KEY=new RegExp(`^${INTENT_PREFIX.replaceAll('.','\\.')}([0-9a-f]{64})\\.([0-9a-f]{32})$`);
const scopePattern=/^[0-9a-f]{64}$/;
const mutable=new Set(['nativeRefreshReceipt','nativeRefreshTerminalReceipt','nativeRefreshLookup','nativeRefreshValidationError','nativeObservation','nativeFenceObservation',
    'nativeSetupObservation','nativeSourceObservation','nativeCatalogObservation','nativePolicyObservation','localDispatchState','nativeSplitReceipt','nativeSplitSnapshot']);
const digest=(value:Buffer|string)=>createHash('sha256').update(value).digest('hex');
const json=(value:unknown)=>JSON.stringify(value);
const normalized=(value:unknown):unknown=>JSON.parse(json(value));
function ordered(value:unknown):unknown{return Array.isArray(value)?value.map(ordered):value!==null&&typeof value==='object'
    ?Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([key,item])=>[key,ordered(item)])):value;}
const immutable=(value:SavedIntent)=>Object.fromEntries(Object.entries(value).filter(([key])=>!mutable.has(key)));
const intentHash=(value:SavedIntent)=>digest(json(ordered(immutable(value))));
type Stamp={dev:bigint;ino:bigint;born:bigint};
type ReadFile={value:unknown;bytes:Buffer;stamp:Stamp;metadata:fs.BigIntStats};
interface Base {schema:1;key:string;generation:string;value:SavedIntent}
interface Entry {base:Base;hash:string;stamp:Stamp;value:SavedIntent;bytes:number;anchors:CapturedFile[];overlay?:{hash:string;stamp:Stamp}}
interface Publication {name:string;hash:string;stamp:Stamp}
type CapturedFile=ScopeRetirementCapture['files'][number];
interface RetirementProof {schema:1;kind:'retire_exact_private_intent';key:string;directory:string;
    root:ScopeRetirementCapture['root'];generation:string;files:CapturedFile[]}
interface Import {schema:1;scope:string;keys:Record<string,string>}
export interface ScopeRetirementCapture {schema:1;scope:string;controlDir:string;namespace:string;directory:string;
    root:{dev:string;ino:string;born:string};import_sha256:string;
    files:{name:string;sha256:string;physical:{dev:string;ino:string;born:string}}[];
    legacy:{key:string;sha256:string;immutable_sha256:string}[]}
export interface ScopeRetirementResult {schema:1;scope:string;operation_id:string;complete:boolean;state:'complete'|'blocked';
    removed_files:string[];remaining_files:string[];blocked:string[];legacy_remaining:string[];late_observed_legacy:{key:string;sha256:string}[];fence_sha256:string}
const fields=(value:unknown,names:string[]):value is Record<string,unknown>=>value!==null&&typeof value==='object'&&!Array.isArray(value)
    &&Object.keys(value).sort().join(',')===[...names].sort().join(',');
const physical=(value:unknown):boolean=>fields(value,['dev','ino','born'])&&Object.values(value).every(item=>typeof item==='string'&&/^(?:0|[1-9][0-9]{0,39})$/.test(item))&&value.ino!=='0'&&value.born!=='0';
export function validateScopeRetirementCapture(input:unknown):ScopeRetirementCapture{
    if(Buffer.byteLength(json(input))>PRIVATE_INTENT_CAPTURE_BYTES||!fields(input,['schema','scope','controlDir','namespace','directory','root','import_sha256','files','legacy'])
        ||input.schema!==1||typeof input.scope!=='string'||!scopePattern.test(input.scope)||typeof input.namespace!=='string'||!/^[0-9a-f]{32}$/.test(input.namespace)
        ||typeof input.controlDir!=='string'||input.controlDir.length>16384||!path.isAbsolute(input.controlDir)
        ||typeof input.directory!=='string'||input.directory.length>16384||!path.isAbsolute(input.directory)||!physical(input.root)
        ||input.scope!==digest(json([input.controlDir,input.namespace]))||typeof input.import_sha256!=='string'||!/^[0-9a-f]{64}$/.test(input.import_sha256)
        ||!Array.isArray(input.files)||input.files.length>PRIVATE_INTENT_CAPTURE_FILES||!Array.isArray(input.legacy)||input.legacy.length>INTENT_LIMIT)throw new ServiceError('private_intent_scope_capture_invalid');
    const names=new Set<string>(),keys=new Set<string>(),nonces=new Set<string>();
    for(const item of input.files){
        if(!fields(item,['name','sha256','physical'])||typeof item.name!=='string'||!new RegExp(`^(?:[bo]-${input.scope}-[0-9a-f]{32}|a-${input.scope}-[0-9a-f]{32}-[a-z][a-z0-9_]{0,63})\\.json$`).test(item.name)
            ||names.has(item.name)||typeof item.sha256!=='string'||!/^[0-9a-f]{64}$/.test(item.sha256)||!physical(item.physical))throw new ServiceError('private_intent_scope_capture_invalid');
        names.add(item.name);nonces.add(item.name.slice(67,99));
    }
    for(const item of input.legacy){
        if(!fields(item,['key','sha256','immutable_sha256'])||typeof item.key!=='string'||nameKey(item.key).scope!==input.scope||keys.has(item.key)
            ||typeof item.sha256!=='string'||!/^[0-9a-f]{64}$/.test(item.sha256)||typeof item.immutable_sha256!=='string'||!/^[0-9a-f]{64}$/.test(item.immutable_sha256))throw new ServiceError('private_intent_scope_capture_invalid');
        keys.add(item.key);nonces.add(item.key.slice(-32));
    }
    if(nonces.size>INTENT_LIMIT)throw new ServiceError('private_intent_scope_capture_invalid');
    return normalized(input) as ScopeRetirementCapture;
}
function stamp(value:fs.BigIntStats):Stamp{return {dev:value.dev,ino:value.ino,born:value.birthtimeNs};}
function sameStamp(a:Stamp,b:Stamp):boolean{return a.dev===b.dev&&a.ino===b.ino&&a.born===b.born;}
function nameKey(value:string):{scope:string;stem:string}{const match=KEY.exec(value);if(!match)throw new ServiceError('private_intent_key_invalid');return {scope:match[1],stem:match[1]+'-'+match[2]};}
function physicalName(value:string):string{
    if(process.platform==='win32')return value.replace(/^\\\\\?\\UNC\\/,'\\\\').replace(/^\\\\\?\\/,'').toLowerCase();
    return value;
}

export class PrivateIntentStore implements IntentStorage {
    private root?:Stamp;
    private scopes=new Set<string>();
    private initialization:Promise<unknown>=Promise.resolve();
    private publications=new Map<string,Stamp>();
    private readRemaining?:number;
    private censusCharges=new WeakMap<Map<string,SavedIntent>,Map<string,number>>();
    constructor(private readonly directory:string,private readonly legacy:IntentStorage,
        private readonly valid:()=>boolean=()=>true,private readonly prepareParent:()=>Promise<void>=async()=>{},
        private readonly physicalReadBytes:number=PRIVATE_INTENT_PHYSICAL_READ_BYTES){
        if(!Number.isSafeInteger(physicalReadBytes)||physicalReadBytes<=0||physicalReadBytes>PRIVATE_INTENT_PHYSICAL_READ_BYTES)throw new ServiceError('private_intent_read_budget_invalid');
    }
    private authorized():void{if(!this.valid())throw new ServiceError('private_intent_authority_changed');}
    private guard():void{
        this.authorized();if(!this.root)throw new ServiceError('private_intent_store_not_initialized');
        const current=fs.lstatSync(this.directory,{bigint:true});
        if(!current.isDirectory()||current.isSymbolicLink()||!sameStamp(stamp(current),this.root)
            ||physicalName(fs.realpathSync(this.directory))!==physicalName(path.resolve(this.directory)))throw new ServiceError('private_intent_directory_changed');
        if(process.platform!=='win32'&&((current.mode&0o077n)!==0n||typeof process.getuid==='function'&&current.uid!==BigInt(process.getuid())))throw new ServiceError('private_intent_directory_not_private');
    }
    private file(name:string):string{return path.join(this.directory,name);}
    private legacyKeys(scope?:string):string[]{
        const result:string[]=[],prefix=INTENT_PREFIX+(scope?scope+'.':'');
        for(const key of this.legacy.keys())if(key.startsWith(prefix)){result.push(key);if(result.length>MAX_FILES)throw new ServiceError('private_intent_legacy_capacity');}
        return result;
    }
    private bounded<T>(action:()=>T):T{
        if(this.readRemaining!==undefined)return action();
        this.readRemaining=this.physicalReadBytes;
        try{return action();}finally{this.readRemaining=undefined;}
    }
    private read(name:string):ReadFile|undefined{
        this.guard();let handle:number;
        try{handle=fs.openSync(this.file(name),fs.constants.O_RDONLY| (fs.constants.O_NOFOLLOW??0));}
        catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return undefined;throw error;}
        try{
            const before=fs.fstatSync(handle,{bigint:true});let named:fs.BigIntStats;
            try{named=fs.lstatSync(this.file(name),{bigint:true});}catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return undefined;throw error;}
            if(!before.isFile()||named.isSymbolicLink()||before.nlink!==1n||before.size>BigInt(name.startsWith('t-')?RETIREMENT_BYTES:MAX_FILE_BYTES)
                ||!sameStamp(stamp(before),stamp(named))||process.platform!=='win32'&&((before.mode&0o077n)!==0n
                    ||typeof process.getuid==='function'&&before.uid!==BigInt(process.getuid())))throw new ServiceError('private_intent_file_unsafe');
            const allocation=Number(before.size)+1;
            if(this.readRemaining!==undefined){if(allocation>this.readRemaining)throw new ServiceError('private_intent_read_budget');this.readRemaining-=allocation;}
            const buffer=Buffer.alloc(allocation);let used=0;
            while(used<buffer.length){const count=fs.readSync(handle,buffer,used,buffer.length-used,null);if(!count)break;used+=count;}
            const bytes=buffer.subarray(0,used),after=fs.fstatSync(handle,{bigint:true});let stillNamed:fs.BigIntStats;
            try{stillNamed=fs.lstatSync(this.file(name),{bigint:true});}catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return undefined;throw error;}
            this.guard();
            if(stillNamed.isSymbolicLink()||!sameStamp(stamp(before),stamp(stillNamed))||!sameStamp(stamp(before),stamp(after))
                ||after.nlink!==1n||before.mode!==after.mode||before.uid!==after.uid||before.size!==after.size
                ||before.mtimeNs!==after.mtimeNs||before.ctimeNs!==after.ctimeNs||BigInt(bytes.length)!==before.size)throw new ServiceError('private_intent_file_changed');
            return {value:JSON.parse(bytes.toString('utf8')),bytes,stamp:stamp(before),metadata:after};
        }finally{fs.closeSync(handle);}
    }
    private names():string[]{
        this.guard();const names:string[]=[],directory=fs.opendirSync(this.directory);
        try{for(let entry=directory.readSync();entry;entry=directory.readSync()){names.push(entry.name);if(names.length>MAX_FILES)throw new ServiceError('private_intent_directory_capacity');}}finally{directory.closeSync();}
        for(const name of names){if(name!=='owned.json'&&!/^[bo]-[0-9a-f]{64}-[0-9a-f]{32}\.json$/.test(name)
            &&!/^a-[0-9a-f]{64}-[0-9a-f]{32}-[a-z][a-z0-9_]{0,63}\.json$/.test(name)
            &&! /^[ir]-[0-9a-f]{64}\.json$/.test(name)&&!/^t-[0-9a-f]{64}-[0-9a-f]{32}\.json$/.test(name)
            &&!/^\.publish-(?:[0-9a-f]{64}-)?[0-9a-f-]{36}$/.test(name))throw new ServiceError('private_intent_unknown_file');}
        return names;
    }
    private syncDirectory():void{
        if(process.platform==='win32')return;
        const descriptor=fs.openSync(this.directory,fs.constants.O_RDONLY);try{fs.fsyncSync(descriptor);}finally{fs.closeSync(descriptor);}
    }
    private temporary(value:unknown,scope?:string):string{
        this.guard();const name='.publish-'+(scope?scope+'-':'')+randomUUID();
        const handle=fs.openSync(this.file(name),'wx',0o600);
        this.publications.set(name,stamp(fs.fstatSync(handle,{bigint:true})));
        try{fs.writeFileSync(handle,json(value));fs.fsyncSync(handle);this.guard();}
        catch(error){fs.closeSync(handle);this.removeTemporary(name);throw error;}
        fs.closeSync(handle);return name;
    }
    private removeTemporary(name:string):void{
        const owned=this.publications.get(name);if(!owned)return;
        try{const current=fs.lstatSync(this.file(name),{bigint:true});if(current.isSymbolicLink()||!sameStamp(owned,stamp(current)))throw new ServiceError('private_intent_stage_changed');fs.unlinkSync(this.file(name));}
        catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}
        finally{this.publications.delete(name);}
    }
    private exclusive(name:string,value:unknown,options:{scope?:string;beforePublish?:(stage:string)=>void;published?:(physical:Stamp)=>void}={}):void{
        const temporary=this.temporary(value,options.scope),physical=this.publications.get(temporary)!;
        try{this.guard();options.beforePublish?.(temporary);fs.linkSync(this.file(temporary),this.file(name));options.published?.(physical);}
        finally{this.removeTemporary(temporary);}
        this.syncDirectory();this.guard();
    }
    private importReceipt(scope:string):Import|undefined{
        const row=this.read('i-'+scope+'.json');if(!row)return undefined;
        const value=row.value as Import;
        if(value?.schema!==1||value.scope!==scope||!value.keys||typeof value.keys!=='object'||Array.isArray(value.keys)
            ||Object.keys(value.keys).length>INTENT_LIMIT||Object.entries(value.keys).some(([key,hash])=>nameKey(key).scope!==scope||!/^[0-9a-f]{64}$/.test(hash)))throw new ServiceError('private_intent_import_invalid');
        return value;
    }
    private scopeFence(scope:string):{capture:ScopeRetirementCapture;operation:string;row:ReadFile}|undefined{
        const row=this.read('r-'+scope+'.json');if(!row)return undefined;
        const value=row.value;
        if(!fields(value,['schema','scope','operation_id','capture'])||value.schema!==1||value.scope!==scope
            ||typeof value.operation_id!=='string'||!/^[0-9a-f]{32}$/.test(value.operation_id))throw new ServiceError('private_intent_scope_capture_crossed');
        const capture=validateScopeRetirementCapture(value.capture);
        if(capture.scope!==scope||capture.directory!==this.directory||!isDeepStrictEqual(capture.root,this.rootBinding()))throw new ServiceError('private_intent_scope_capture_crossed');
        const imported=this.read('i-'+scope+'.json');if(!imported||digest(imported.bytes)!==capture.import_sha256)throw new ServiceError('private_intent_scope_capture_changed');
        return {capture,operation:value.operation_id,row};
    }
    private revokedLegacy(key:string,input:unknown,fence=this.scopeFence(nameKey(key).scope)):boolean{
        if(!fence)return false;
        // The accepted Genesis boundary revokes the entire predecessor request
        // namespace, including valid writes queued by an old editor after the
        // original snapshot. This never adopts another scope or malformed row.
        validateStoredIntent(key,input,fence.capture.namespace);return true;
    }
    private attachCapturedRoot(capture:ScopeRetirementCapture):void{
        this.authorized();
        if(!this.root){
            const current=fs.lstatSync(this.directory,{bigint:true});
            if(!current.isDirectory()||current.isSymbolicLink()||physicalName(fs.realpathSync(this.directory))!==physicalName(path.resolve(this.directory))
                ||!isDeepStrictEqual({dev:String(current.dev),ino:String(current.ino),born:String(current.birthtimeNs)},capture.root))throw new ServiceError('private_intent_scope_capture_crossed');
            this.root=stamp(current);
            try{this.guard();const owned=this.read('owned.json');if(!owned||!isDeepStrictEqual(owned.value,{schema:1,kind:'airk-private-intent-store'}))throw new ServiceError('private_intent_directory_not_owned');}
            catch(error){this.root=undefined;throw error;}
        }
        this.guard();
        if(capture.directory!==this.directory||!isDeepStrictEqual(capture.root,this.rootBinding()))throw new ServiceError('private_intent_scope_capture_crossed');
    }
    private anchorName(key:string,slot:string):string{return 'a-'+nameKey(key).stem+'-'+slot+'.json';}
    private anchorEvidence(key:string,base:Base,hash:string,slot:string,row:ReadFile):AcceptedIntentEvidence{
        const value=row.value as {schema:number;key:string;base_sha256:string;generation:string;slot:string;evidence:unknown};
        if(!acceptedIntentSlots(base.value.intent).includes(slot)||!fields(value,['schema','key','base_sha256','generation','slot','evidence'])
            ||value.schema!==1||value.key!==key||value.base_sha256!==hash||value.generation!==base.generation||value.slot!==slot)throw new ServiceError('private_intent_accepted_evidence_changed');
        const evidence={slot,value:value.evidence};mergeAcceptedIntentEvidence(base.value,[evidence]);return evidence;
    }
    private anchors(key:string,base:Base,hash:string):{evidence:AcceptedIntentEvidence;file:CapturedFile}[]{
        const result:{evidence:AcceptedIntentEvidence;file:CapturedFile}[]=[];
        for(const slot of acceptedIntentSlots(base.value.intent)){
            const name=this.anchorName(key,slot),row=this.read(name);if(!row)continue;
            const evidence=this.anchorEvidence(key,base,hash,slot,row);
            result.push({evidence,file:this.captureFile(name,row)});
        }
        return result;
    }
    private publicationFence(key:string,expected:Entry,ownStage?:string):void{
        const current=this.read('b-'+nameKey(key).stem+'.json');
        if(!current||digest(current.bytes)!==expected.hash||!sameStamp(current.stamp,expected.stamp))throw new ServiceError('private_intent_base_changed');
        if(this.read('r-'+nameKey(key).scope+'.json'))throw new ServiceError('private_intent_scope_retired');
        if(this.read(this.retirementName(key)))throw new ServiceError('private_intent_retirement_in_progress');
        if(ownStage&&this.livePublisher(key,undefined,this.names().filter(name=>name!==ownStage)))throw new ServiceError('private_intent_observation_in_progress');
    }
    private projectedCapacity(key:string,proposed:SavedIntent,before:Entry,evidence:readonly AcceptedIntentEvidence[]):SavedIntent{
        const effective=mergeAcceptedIntentEvidence(proposed,evidence),fallback=mergeAcceptedIntentEvidence(before.value,[...evidence,...selectAcceptedIntentEvidence(proposed)]);
        validateStoredIntent(key,effective);validateStoredIntent(key,fallback);
        const roster=this.roster(),other=this.charged(roster,key);
        if(roster.size>INTENT_LIMIT||other+Math.max(chargedRecordBytes(effective),chargedRecordBytes(fallback))>INTENT_MAX_TOTAL_BYTES)throw new ServiceError('private_intent_global_capacity');
        return effective;
    }
    private publishAnchors(key:string,before:Entry,value:SavedIntent,owned:Publication[]):SavedIntent{
        if(acceptedIntentSlots(value.intent).length===0)return validateStoredIntent(key,mergeAcceptedIntentEvidence(value,[]));
        let existing=this.anchors(key,before.base,before.hash);
        // Recover accepted evidence from an interrupted first publication only
        // when its slot has no durable anchor. A compatible newer lookup in an
        // already anchored slot remains current progress, not a replacement for
        // that slot's immutable fallback after the lookup becomes unknown.
        const inherited=selectAcceptedIntentEvidence(before.value).filter(evidence=>!existing.some(item=>item.evidence.slot===evidence.slot));
        let effective=this.projectedCapacity(key,value,before,[...existing.map(item=>item.evidence),...inherited]);
        const incoming=[...inherited,...selectAcceptedIntentEvidence(value)],{scope}=nameKey(key);
        for(const evidence of incoming){
            const prior=existing.find(item=>item.evidence.slot===evidence.slot);
            if(prior){mergeAcceptedIntentEvidence(effective,[prior.evidence,evidence]);continue;}
            if(existing.some(item=>acceptedIntentEvidenceDominates(effective,item.evidence,evidence)))continue;
            const name=this.anchorName(key,evidence.slot),anchor={schema:1,key,base_sha256:before.hash,generation:before.base.generation,slot:evidence.slot,evidence:evidence.value};
            this.publicationFence(key,before);
            try{this.exclusive(name,anchor,{scope,beforePublish:stage=>{
                const current=this.entry(key);if(!current)throw new ServiceError('private_intent_base_changed');
                this.projectedCapacity(key,value,current,[...this.anchors(key,before.base,before.hash).map(item=>item.evidence),evidence]);
                this.publicationFence(key,before,stage);
            },
                published:physical=>owned.push({name,hash:digest(json(anchor)),stamp:physical})});}
            catch(error){if((error as NodeJS.ErrnoException).code!=='EEXIST')throw error;}
            this.publicationFence(key,before);
            existing=this.anchors(key,before.base,before.hash);
            effective=this.projectedCapacity(key,value,before,existing.map(item=>item.evidence));
        }
        // Only a validated stronger, later fixed slot may compact an earlier
        // anchor. Readers walk that same order, so disappearance always leaves
        // a durable compatible anchor they have already seen or will see next.
        const slots=acceptedIntentSlots(before.base.value.intent);
        for(const weaker of existing){
            const stronger=existing.find(item=>slots.indexOf(item.evidence.slot)>slots.indexOf(weaker.evidence.slot)
                &&acceptedIntentEvidenceDominates(effective,item.evidence,weaker.evidence));
            if(stronger){this.publicationFence(key,before);this.removeCaptured(weaker.file);}
        }
        return effective;
    }
    private rollbackAnchors(key:string,before:Pick<Entry,'hash'|'stamp'>,owned:Publication[]):void{
        const current=this.read('b-'+nameKey(key).stem+'.json');
        const retiring=this.read(this.retirementName(key)),scopeFence=this.read('r-'+nameKey(key).scope+'.json');
        if(current&&digest(current.bytes)===before.hash&&sameStamp(current.stamp,before.stamp)&&!retiring&&!scopeFence)return;
        const retained=new Set<string>();
        if(retiring)for(const file of this.proof(retiring.value,this.retirementName(key)).files)retained.add(file.name);
        if(scopeFence){const capture=validateScopeRetirementCapture((scopeFence.value as {capture:unknown}).capture);for(const file of capture.files)retained.add(file.name);}
        for(const publication of owned){if(retained.has(publication.name))continue;const row=this.read(publication.name);if(row&&digest(row.bytes)===publication.hash&&sameStamp(row.stamp,publication.stamp))this.removeCaptured(this.captureFile(publication.name,row));}
    }
    private retirementName(key:string):string{return 't-'+nameKey(key).stem+'.json';}
    private rootBinding():ScopeRetirementCapture['root']{return {dev:String(this.root!.dev),ino:String(this.root!.ino),born:String(this.root!.born)};}
    private captureFile(name:string,row:{bytes:Buffer;stamp:Stamp}):CapturedFile{return {name,sha256:digest(row.bytes),physical:{dev:String(row.stamp.dev),ino:String(row.stamp.ino),born:String(row.stamp.born)}};}
    private matchesFile(row:{bytes:Buffer;stamp:Stamp},expected:CapturedFile):boolean{return digest(row.bytes)===expected.sha256
        &&sameStamp(row.stamp,{dev:BigInt(expected.physical.dev),ino:BigInt(expected.physical.ino),born:BigInt(expected.physical.born)});}
    private proof(input:unknown,name:string):RetirementProof{
        if(Buffer.byteLength(json(input))>RETIREMENT_BYTES||!fields(input,['schema','kind','key','directory','root','generation','files'])
            ||input.schema!==1||input.kind!=='retire_exact_private_intent'||typeof input.key!=='string'||this.retirementName(input.key)!==name
            ||input.directory!==this.directory||!physical(input.root)||!isDeepStrictEqual(input.root,this.rootBinding())
            ||typeof input.generation!=='string'||!/^[0-9a-f-]{36}$/.test(input.generation)||!Array.isArray(input.files)||input.files.length<1||input.files.length>RETIREMENT_FILES)throw new ServiceError('private_intent_retirement_proof_invalid');
        const stem=nameKey(input.key).stem;
        const seen=new Set<string>();
        for(let position=0;position<input.files.length;position++){
            const file=input.files[position];
            if(!fields(file,['name','sha256','physical'])||typeof file.name!=='string'||seen.has(file.name)
                ||(position===0?file.name!=='b-'+stem+'.json':file.name!=='o-'+stem+'.json'&&!new RegExp(`^a-${stem}-[a-z][a-z0-9_]{0,63}\\.json$`).test(file.name))
                ||typeof file.sha256!=='string'||!/^[0-9a-f]{64}$/.test(file.sha256)||!physical(file.physical))throw new ServiceError('private_intent_retirement_proof_invalid');
            seen.add(file.name);
        }
        return normalized(input) as RetirementProof;
    }
    private cachedRead(name:string,cache?:Map<string,ReadFile|undefined>):ReadFile|undefined{
        if(!cache)return this.read(name);if(!cache.has(name))cache.set(name,this.read(name));return cache.get(name);
    }
    private livePublisher(key:string,cache?:Map<string,ReadFile|undefined>,names=this.names()):boolean{
        const scope=nameKey(key).scope;
        for(const name of names.filter(name=>name.startsWith('.publish-'))){
            const selected=/^\.publish-([0-9a-f]{64})-[0-9a-f-]{36}$/.exec(name)?.[1];
            if(selected&&selected!==scope)continue;
            const row=this.cachedRead(name,cache);if(!row)continue;const value=row.value as {key?:unknown};
            if(!selected||typeof value?.key!=='string'||value.key===key)return true;
        }
        return false;
    }
    private checkRetirement(proof:RetirementProof,cache?:Map<string,ReadFile|undefined>):void{
            for(const file of proof.files){const row=this.cachedRead(file.name,cache);if(row&&!this.matchesFile(row,file))throw new ServiceError('private_intent_retirement_changed');
            if(row&&file.name.startsWith('b-')){
                const base=row.value as Base;if(base?.schema!==1||base.key!==proof.key||base.generation!==proof.generation)throw new ServiceError('private_intent_retirement_changed');
                validateStoredIntent(proof.key,base.value);
            }
        }
        const known=new Set(proof.files.map(file=>file.name)),stem=nameKey(proof.key).stem;
        for(const name of this.names().filter(name=>name==='o-'+stem+'.json'||name.startsWith('a-'+stem+'-')))if(!known.has(name))throw new ServiceError('private_intent_retirement_changed');
    }
    private overlayValue(key:string,base:Base,hash:string,input:unknown):SavedIntent{
        const observation=input as {schema:number;key?:string;base_sha256:string;changes:Record<string,unknown>;removed:string[]};
        if(observation?.schema!==1||observation.key!==undefined&&observation.key!==key||observation.base_sha256!==hash
            ||!observation.changes||typeof observation.changes!=='object'||Array.isArray(observation.changes)||!Array.isArray(observation.removed)
            ||observation.removed.some(field=>typeof field!=='string'||!mutable.has(field))||Object.keys(observation.changes).some(field=>!mutable.has(field)))throw new ServiceError('private_intent_observation_invalid');
        const merged={...base.value,...observation.changes} as unknown as Record<string,unknown>;for(const field of observation.removed)delete merged[field];
        const value=validateStoredIntent(key,merged);if(!isDeepStrictEqual(immutable(base.value),immutable(value)))throw new ServiceError('private_intent_request_changed');return value;
    }
    private removeCaptured(file:CapturedFile,cache?:Map<string,ReadFile|undefined>):void{
        const row=this.cachedRead(file.name,cache);
        let current:fs.BigIntStats;try{current=fs.lstatSync(this.file(file.name),{bigint:true});}catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return;throw error;}
        if(!row||current.isSymbolicLink()||!current.isFile()||!sameStamp(stamp(current),row.stamp)||current.nlink!==1n
            ||current.size!==row.metadata.size||current.mode!==row.metadata.mode||current.uid!==row.metadata.uid
            ||current.mtimeNs!==row.metadata.mtimeNs||current.ctimeNs!==row.metadata.ctimeNs)throw new ServiceError('private_intent_retirement_changed');
        if(!this.matchesFile(row,file))throw new ServiceError('private_intent_retirement_changed');
        this.guard();fs.unlinkSync(this.file(file.name));this.syncDirectory();
        cache?.set(file.name,undefined);
    }
    private settleRetirement(proof:RetirementProof,cache=new Map<string,ReadFile|undefined>()):void{
        this.checkRetirement(proof,cache);
        if(this.livePublisher(proof.key,cache))throw new ServiceError('private_intent_retirement_in_progress');
        for(const file of proof.files)this.removeCaptured(file,cache);
        if(this.livePublisher(proof.key,cache))throw new ServiceError('private_intent_retirement_in_progress');
        // A late observer is fenced before publication and before ACK. Never
        // remove a changed or newly introduced observation under an old proof.
        for(const file of proof.files){try{fs.lstatSync(this.file(file.name));throw new ServiceError('private_intent_retirement_unconfirmed');}catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}}
        const name=this.retirementName(proof.key),row=this.cachedRead(name,cache);
        if(row){if(!isDeepStrictEqual(this.proof(row.value,name),proof))throw new ServiceError('private_intent_retirement_proof_changed');this.removeCaptured(this.captureFile(name,row),cache);}
    }
    private recoverRetirements():void{
        this.bounded(()=>{
            const names=this.names(),cache=new Map<string,ReadFile|undefined>(),pending:{proof:RetirementProof;name:string}[]=[],scopes:{capture:ScopeRetirementCapture;operation:string}[]=[];
            for(const name of names.filter(name=>name.startsWith('t-'))){const row=this.cachedRead(name,cache);if(row)pending.push({name,proof:this.proof(row.value,name)});}
            for(const name of names.filter(name=>name.startsWith('r-'))){
                const row=this.cachedRead(name,cache);if(!row)continue;const fence=row.value as {schema:number;scope:string;operation_id:string;capture:unknown};
                if(!fields(fence,['schema','scope','operation_id','capture']))throw new ServiceError('private_intent_scope_capture_crossed');
                const capture=validateScopeRetirementCapture(fence.capture);
                if(fence.schema!==1||fence.scope!==capture.scope||name!=='r-'+capture.scope+'.json'||!/^[0-9a-f]{32}$/.test(fence.operation_id)
                    ||capture.directory!==this.directory||!isDeepStrictEqual(capture.root,this.rootBinding()))throw new ServiceError('private_intent_scope_capture_crossed');
                const imported=this.cachedRead('i-'+capture.scope+'.json',cache);if(!imported||digest(imported.bytes)!==capture.import_sha256)throw new ServiceError('private_intent_scope_capture_changed');
                scopes.push({capture,operation:fence.operation_id});
            }
            // Validate the entire bounded proof roster and every present
            // preimage before any recovery unlink.
            for(const {proof} of pending){this.checkRetirement(proof,cache);if(this.livePublisher(proof.key,cache,names))throw new ServiceError('private_intent_retirement_in_progress');}
            const ordinary=new Set(pending.map(item=>item.proof.key)),orphans:CapturedFile[]=[];
            for(const {capture} of scopes){
                const expected=new Set(capture.files.map(file=>file.name));
                for(const name of names.filter(name=>new RegExp(`^(?:[bo]-${capture.scope}-[0-9a-f]{32}|a-${capture.scope}-[0-9a-f]{32}-[a-z][a-z0-9_]{0,63})\\.json$`).test(name))){
                    const key=INTENT_PREFIX+capture.scope+'.'+name.slice(67,99);if(ordinary.has(key))continue;
                    if(!expected.has(name))throw new ServiceError('private_intent_retirement_changed');
                }
                for(const file of capture.files){const key=INTENT_PREFIX+capture.scope+'.'+file.name.slice(67,99);if(ordinary.has(key))continue;
                    const row=this.cachedRead(file.name,cache);if(row&&!this.matchesFile(row,file))throw new ServiceError('private_intent_retirement_changed');}
            }
            for(const {capture} of scopes)for(const file of capture.files.filter(file=>/^[oa]-/.test(file.name))){
                const key=INTENT_PREFIX+capture.scope+'.'+file.name.slice(67,99);if(ordinary.has(key))continue;
                let base=false;try{const current=fs.lstatSync(this.file('b-'+file.name.slice(2,99)+'.json'));if(!current.isFile()||current.isSymbolicLink())throw new ServiceError('private_intent_retirement_changed');base=true;}catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}
                if(base)continue;
                const row=this.cachedRead(file.name,cache);if(!row)continue;
                if(!this.matchesFile(row,file)||this.livePublisher(key,cache,names)||this.cachedRead(this.retirementName(key),cache))throw new ServiceError('private_intent_retirement_changed');
                orphans.push(file);
            }
            for(const {proof} of pending)this.settleRetirement(proof,cache);
            for(const file of orphans)this.removeCaptured(file,cache);
        });
    }
    private entry(key:string):Entry|undefined{
        const {stem}=nameKey(key),row=this.read('b-'+stem+'.json');if(!row)return undefined;
        const base=row.value as Base;
        if(base?.schema!==1||base.key!==key||typeof base.generation!=='string'||!/^[0-9a-f-]{36}$/.test(base.generation))throw new ServiceError('private_intent_base_invalid');
        let value=validateStoredIntent(key,base.value);const hash=digest(row.bytes);
        const overlay=this.read('o-'+stem+'.json');
        if(overlay)value=this.overlayValue(key,base,hash,overlay.value);
        const anchors=this.anchors(key,base,hash);value=mergeAcceptedIntentEvidence(value,anchors.map(item=>item.evidence));validateStoredIntent(key,value);
        const payloadBytes=Buffer.byteLength(json(value));
        if(payloadBytes>maximumRecordBytes(value.intent))throw new ServiceError('private_intent_record_capacity');
        return {base,hash,stamp:row.stamp,value,bytes:payloadBytes,anchors:anchors.map(item=>item.file),...(overlay?{overlay:{hash:digest(overlay.bytes),stamp:overlay.stamp}}:{})};
    }
    /** Includes unimported legacy scopes in the global count and byte budget. */
    private roster():Map<string,SavedIntent>{return this.bounded(()=>this.census());}
    private census():Map<string,SavedIntent>{
        const result=new Map<string,SavedIntent>(),entries=new Map<string,Entry>(),charges=new Map<string,number>(),receipts=new Map<string,Import|undefined>(),fences=new Map<string,ReturnType<PrivateIntentStore['scopeFence']>>();
        const names=this.names();
        const owned=this.read('owned.json');if(!owned||!isDeepStrictEqual(owned.value,{schema:1,kind:'airk-private-intent-store'}))throw new ServiceError('private_intent_directory_not_owned');
        // Even invisible late overlays and other-scope import receipts must be
        // bounded private regular files; unknown state is never swept away.
        const baseNames=new Set(names.filter(name=>name.startsWith('b-')));
        for(const name of names)if(/^[oa]-/.test(name)&&!baseNames.has('b-'+name.slice(2,99)+'.json')){
            const row=this.read(name),proofName='t-'+name.slice(2,99)+'.json',retirement=this.read(proofName);
            if(!row||!retirement)throw new ServiceError('private_intent_orphan_requires_review');
            const proof=this.proof(retirement.value,proofName),expected=proof.files.find(file=>file.name===name);
            if(!expected||!this.matchesFile(row,expected))throw new ServiceError('private_intent_orphan_requires_review');
        }
        for(const name of names)if(name.startsWith('.publish-'))this.read(name); // Other windows' stages confer no request authority.
        for(const name of names)if(name.startsWith('i-')){const scope=name.slice(2,-5);receipts.set(scope,this.importReceipt(scope));}
        for(const name of names)if(name.startsWith('b-')){
            const key=INTENT_PREFIX+name.slice(2,66)+'.'+name.slice(67,99),entry=this.entry(key);
            if(!entry)continue;entries.set(key,entry);result.set(key,entry.value);charges.set(key,Math.max(entry.bytes,chargedRecordBytes(entry.value)));
        }
        for(const name of names.filter(name=>name.startsWith('a-'))){
            const key=INTENT_PREFIX+name.slice(2,66)+'.'+name.slice(67,99),entry=entries.get(key);if(!entry)continue;
            const slot=name.slice(100,-5);if(!acceptedIntentSlots(entry.base.value.intent).includes(slot))throw new ServiceError('private_intent_accepted_evidence_changed');
            if(!entry.anchors.some(file=>file.name===name)){
                const row=this.read(name);if(!row)continue;
                const effective=mergeAcceptedIntentEvidence(entry.value,[this.anchorEvidence(key,entry.base,entry.hash,slot,row)]);validateStoredIntent(key,effective);
                result.set(key,effective);charges.set(key,chargedRecordBytes(effective));
            }
        }
        const legacyKeys=this.legacyKeys();
        for(const key of legacyKeys){
            const {scope}=nameKey(key),value=validateStoredIntent(key,this.legacy.get(key));
            if(!fences.has(scope))fences.set(scope,this.scopeFence(scope));
            if(this.revokedLegacy(key,value,fences.get(scope)))continue;
            let imported=receipts.get(scope);if(!receipts.has(scope)){imported=this.importReceipt(scope);receipts.set(scope,imported);}
            if(imported){if(imported.keys[key]!==intentHash(value))throw new ServiceError('private_intent_legacy_outcome_requires_review');continue;}
            const existing=result.get(key);if(existing&&!isDeepStrictEqual(immutable(existing),immutable(value)))throw new ServiceError('private_intent_legacy_request_changed');
            if(!existing){result.set(key,value);charges.set(key,chargedRecordBytes(value));}
        }
        // Reads retain racing over-admission records for receipt reconciliation.
        // Mutation admission checks the existing global limits separately.
        this.censusCharges.set(result,charges);return result;
    }
    private charged(roster:Map<string,SavedIntent>,except?:string):number{
        const charges=this.censusCharges.get(roster);if(!charges)throw new ServiceError('private_intent_census_missing');
        let total=0;for(const [key,value] of charges)if(key!==except)total+=value;return total;
    }
    initialize(scope:string):Promise<void>{
        const next=this.initialization.then(async()=>{
            this.authorized();if(!scopePattern.test(scope))throw new ServiceError('private_intent_scope_invalid');
            if(!this.root){
                await this.prepareParent();this.authorized();
                if(!path.isAbsolute(this.directory))throw new ServiceError('private_intent_directory_invalid');
                let created=false;
                try{fs.mkdirSync(this.directory,{mode:0o700});created=true;}catch(error){if((error as NodeJS.ErrnoException).code!=='EEXIST')throw error;}
                const before=fs.lstatSync(this.directory,{bigint:true});
                if(!before.isDirectory()||before.isSymbolicLink()||before.ino===0n||before.birthtimeNs<=0n
                    ||physicalName(fs.realpathSync(this.directory))!==physicalName(path.resolve(this.directory)))throw new ServiceError('private_intent_directory_unsafe');
                try{
                    if(created){const guard=await restrictWindowsDirectory(this.directory,undefined,this.valid);guard.assertCurrent();}
                    this.root=stamp(before);this.guard();
                    if(created){this.exclusive('owned.json',{schema:1,kind:'airk-private-intent-store'});}
                    const owned=this.read('owned.json');if(!owned||!isDeepStrictEqual(owned.value,{schema:1,kind:'airk-private-intent-store'}))throw new ServiceError('private_intent_directory_not_owned');
                }catch(error){
                    this.root=undefined;
                    if(created){
                        // Only our exact still-empty new directory can be
                        // removed after interrupted ACL/authority setup.
                        const current=fs.lstatSync(this.directory,{bigint:true});
                        if(current.isDirectory()&&!current.isSymbolicLink()&&sameStamp(stamp(current),stamp(before))){
                            const entries=fs.opendirSync(this.directory);let empty:boolean;try{empty=entries.readSync()===null;}finally{entries.closeSync();}
                            if(empty)fs.rmdirSync(this.directory);
                        }
                    }
                    throw error;
                }
            }
            this.recoverRetirements();
            const captured:Record<string,string>={};const originals=new Map<string,SavedIntent>();
            this.roster();
            for(const key of this.legacyKeys()){
                if(nameKey(key).scope===scope){const value=validateStoredIntent(key,this.legacy.get(key));if(this.revokedLegacy(key,value))continue;captured[key]=intentHash(value);originals.set(key,value);}
            }
            if(originals.size>INTENT_LIMIT)throw new ServiceError('private_intent_import_capacity');
            const prior=this.importReceipt(scope);
            if(!prior){
                for(const [key,value] of originals){
                    if(!this.entry(key))this.createBase(key,value);else if(!isDeepStrictEqual(immutable(this.entry(key)!.value),immutable(value)))throw new ServiceError('private_intent_import_conflict');
                    const current=this.entry(key);if(!current)throw new ServiceError('private_intent_import_readback_failed');
                    this.publishAnchors(key,current,value,[]);
                }
                for(const [key,value] of originals){if(!isDeepStrictEqual(this.entry(key)?.value,mergeAcceptedIntentEvidence(value,[])))throw new ServiceError('private_intent_import_readback_failed');}
                const receipt:Import={schema:1,scope,keys:captured};
                try{this.exclusive('i-'+scope+'.json',receipt,{scope});}catch(error){if((error as NodeJS.ErrnoException).code!=='EEXIST'||!isDeepStrictEqual(this.importReceipt(scope),receipt))throw error;}
            }
            this.scopes.add(scope);this.roster();this.guard();
        });
        this.initialization=next.catch(()=>undefined);return next;
    }
    async captureScope(controlDir:string,namespace:string):Promise<ScopeRetirementCapture>{
        const scope=digest(json([controlDir,namespace]));await this.initialize(scope);
        return this.bounded(()=>{
            const existing=this.read('r-'+scope+'.json');
            if(existing){const fence=existing.value as {capture:unknown};const capture=validateScopeRetirementCapture(fence.capture);
                if(capture.controlDir!==controlDir||capture.namespace!==namespace)throw new ServiceError('private_intent_scope_capture_crossed');return capture;}
            const imported=this.read('i-'+scope+'.json');if(!imported)throw new ServiceError('private_intent_import_unconfirmed');
            const files:ScopeRetirementCapture['files']=[],rows=new Map<string,ReadFile>();
            const records=new Map<string,{base:Base;hash:string;value:SavedIntent;evidence:AcceptedIntentEvidence[]}>();
            const names=this.names().filter(name=>new RegExp(`^(?:[bo]-${scope}-[0-9a-f]{32}|a-${scope}-[0-9a-f]{32}-[a-z][a-z0-9_]{0,63})\\.json$`).test(name));
            for(const name of names.filter(name=>name.startsWith('b-')).sort()){
                const row=this.read(name);if(!row)throw new ServiceError('private_intent_capture_changed');rows.set(name,row);
                const key=INTENT_PREFIX+scope+'.'+name.slice(67,99),base=row.value as Base;
                if(base?.schema!==1||base.key!==key||typeof base.generation!=='string'||!/^[0-9a-f-]{36}$/.test(base.generation))throw new ServiceError('private_intent_base_invalid');
                records.set(key,{base,hash:digest(row.bytes),value:validateStoredIntent(key,base.value,namespace),evidence:[]});
            }
            for(const name of names.filter(name=>!name.startsWith('b-')).sort()){
                const row=this.read(name);if(!row)throw new ServiceError('private_intent_capture_changed');rows.set(name,row);
                const key=INTENT_PREFIX+scope+'.'+name.slice(67,99),entry=records.get(key);
                if(!entry)throw new ServiceError('private_intent_orphan_requires_review');
                if(name.startsWith('o-'))entry.value=this.overlayValue(key,entry.base,entry.hash,row.value);
                else entry.evidence.push(this.anchorEvidence(key,entry.base,entry.hash,name.slice(100,-5),row));
            }
            for(const [key,entry] of records)validateStoredIntent(key,mergeAcceptedIntentEvidence(entry.value,entry.evidence),namespace);
            // Capture exactly the bodies validated above, with one physical
            // read per file. A changed/missing path refuses rather than being
            // silently admitted by a later prefix-only hash pass.
            for(const name of names.sort()){
                const row=rows.get(name)!;let current:fs.BigIntStats;
                try{current=fs.lstatSync(this.file(name),{bigint:true});}catch{throw new ServiceError('private_intent_capture_changed');}
                if(!current.isFile()||current.isSymbolicLink()||current.nlink!==1n||!sameStamp(stamp(current),row.stamp)
                    ||current.size!==row.metadata.size||current.mtimeNs!==row.metadata.mtimeNs||current.ctimeNs!==row.metadata.ctimeNs
                    ||current.mode!==row.metadata.mode||current.uid!==row.metadata.uid)throw new ServiceError('private_intent_capture_changed');
                files.push(this.captureFile(name,row));
            }
            const legacy:ScopeRetirementCapture['legacy']=[];
            for(const key of this.legacyKeys(scope).sort()){
                const value=validateStoredIntent(key,this.legacy.get(key),namespace);legacy.push({key,sha256:digest(json(ordered(value))),immutable_sha256:intentHash(value)});
            }
            return validateScopeRetirementCapture({schema:1,scope,controlDir,namespace,directory:this.directory,
                root:{dev:String(this.root!.dev),ino:String(this.root!.ino),born:String(this.root!.born)},import_sha256:digest(imported.bytes),files,legacy});
        });
    }
    /** The host supplies the accepted native reset operation only after its
     * selected Genesis/request-authority boundary has been checked. */
    async retireCapturedScope(input:ScopeRetirementCapture,operationId:string):Promise<ScopeRetirementResult>{
        const capture=validateScopeRetirementCapture(input);
        if(!/^[0-9a-f]{32}$/.test(operationId))throw new ServiceError('private_intent_scope_capture_crossed');
        this.attachCapturedRoot(capture);
        if(capture.directory!==this.directory
            ||!sameStamp({dev:BigInt(capture.root.dev),ino:BigInt(capture.root.ino),born:BigInt(capture.root.born)},this.root!))throw new ServiceError('private_intent_scope_capture_crossed');
        const fenceName='r-'+capture.scope+'.json',expectedFence={schema:1,scope:capture.scope,operation_id:operationId,capture};
        let fence=this.read(fenceName),hadFence=!!fence;
        if(fence&&!isDeepStrictEqual(fence.value,expectedFence))throw new ServiceError('private_intent_scope_retirement_conflict');
        this.recoverRetirements();
        const preflight=():string[]=>this.bounded(()=>{
            const blocked:string[]=[],imported=this.read('i-'+capture.scope+'.json');
            if(!imported||digest(imported.bytes)!==capture.import_sha256)blocked.push('legacy_import_receipt_changed');
            const files=new Map(capture.files.map(value=>[value.name,value]));
            const names=this.names();
            for(const name of names.filter(name=>name.startsWith('.publish-'))){
                const scope=/^\.publish-([0-9a-f]{64})-[0-9a-f-]{36}$/.exec(name)?.[1];
                // A publisher registers its scope before checking the fence.
                // Unattributed stages never receive a foreign-scope exemption.
                if(!scope||scope===capture.scope)blocked.push(name+':publisher_in_flight');
            }
            for(const name of names.filter(name=>new RegExp(`^t-${capture.scope}-[0-9a-f]{32}\\.json$`).test(name)))blocked.push(name+':retirement_in_progress');
            const present=names.filter(name=>new RegExp(`^(?:[bo]-${capture.scope}-[0-9a-f]{32}|a-${capture.scope}-[0-9a-f]{32}-[a-z][a-z0-9_]{0,63})\\.json$`).test(name)),presentSet=new Set(present);
            for(const name of present){
                const expected=files.get(name),row=this.read(name);
                if(!expected||!row||digest(row.bytes)!==expected.sha256||!sameStamp(row.stamp,{dev:BigInt(expected.physical.dev),ino:BigInt(expected.physical.ino),born:BigInt(expected.physical.born)}))blocked.push(name);
            }
            if(!hadFence)for(const expected of capture.files)if(!presentSet.has(expected.name))blocked.push(expected.name+':missing_before_fence');
            const legacy=new Map(capture.legacy.map(value=>[value.key,value])),retired=this.scopeFence(capture.scope);
            for(const key of this.legacyKeys(capture.scope)){
                try{const value=validateStoredIntent(key,this.legacy.get(key),capture.namespace),expected=legacy.get(key);
                    if(this.revokedLegacy(key,value,retired))continue;
                    if(!expected||digest(json(ordered(value)))!==expected.sha256||intentHash(value)!==expected.immutable_sha256)blocked.push(key);
                }catch{blocked.push(key+':invalid_predecessor_legacy');}
            }
            if(!hadFence)for(const expected of capture.legacy)if(this.legacy.get(expected.key)===undefined)blocked.push(expected.key+':missing_before_fence');
            return [...new Set(blocked)].sort();
        });
        let blocked=preflight();
        const prefix=INTENT_PREFIX+capture.scope+'.';
        const persistentBlockers=blocked.filter(value=>!value.startsWith(prefix)||value.endsWith(':invalid_predecessor_legacy'));
        // Publish an immutable operation-bound fence even for a blocked
        // preimage, so other windows cannot admit new work into this old scope.
        if(!fence){
            try{this.exclusive(fenceName,expectedFence,{scope:capture.scope});}catch(error){if((error as NodeJS.ErrnoException).code!=='EEXIST')throw error;}
            fence=this.read(fenceName);if(!fence||!isDeepStrictEqual(fence.value,expectedFence))throw new ServiceError('private_intent_scope_retirement_conflict');
        }
        hadFence=true;
        // Only legacy observation blockers may disappear after namespace
        // revocation. Re-evaluate the physical/malformed proofs under the same
        // bounded pass instead of retaining obsolete pre-fence observations.
        blocked=[...new Set([...persistentBlockers,...preflight()])].sort();
        const lateObserved:{key:string;sha256:string}[]=[],observedLegacy:{key:string;sha256:string}[]=[];
        const capturedLegacy=new Map(capture.legacy.map(row=>[row.key,row.sha256]));
        for(const key of this.legacyKeys(capture.scope).sort()){
            try{const value=validateStoredIntent(key,this.legacy.get(key),capture.namespace);this.revokedLegacy(key,value);
                const sha256=digest(json(ordered(value)));observedLegacy.push({key,sha256});
                if(capturedLegacy.get(key)!==sha256)lateObserved.push({key,sha256});
            }catch{blocked.push(key+':invalid_predecessor_legacy');}
            if(observedLegacy.length>MAX_FILES)throw new ServiceError('private_intent_legacy_capacity');
        }
        if(!blocked.length){
            // Bases disappear first. A late observation cannot restore visible
            // pending work because replace never creates a request base.
            for(const expected of [...capture.files].sort((a,b)=>Number(!a.name.startsWith('b-'))-Number(!b.name.startsWith('b-'))||a.name.localeCompare(b.name))){
                const row=this.read(expected.name);if(!row)continue;
                if(digest(row.bytes)!==expected.sha256||!sameStamp(row.stamp,{dev:BigInt(expected.physical.dev),ino:BigInt(expected.physical.ino),born:BigInt(expected.physical.born)})){blocked.push(expected.name);break;}
                this.guard();fs.unlinkSync(this.file(expected.name));this.syncDirectory();
            }
            if(!blocked.length)for(const expected of observedLegacy){
                const current=this.legacy.get(expected.key);if(current===undefined)continue;
                try{if(digest(json(ordered(validateStoredIntent(expected.key,current,capture.namespace))))!==expected.sha256){blocked.push(expected.key+':legacy_changed_during_removal');continue;}}
                catch{blocked.push(expected.key+':legacy_changed_during_removal');continue;}
                this.authorized();await this.legacy.update(expected.key,undefined);this.guard();
                if(this.legacy.get(expected.key)!==undefined)blocked.push(expected.key+':legacy_removal_unconfirmed');
            }
        }
        blocked=[...new Set([...blocked,...preflight()])].sort();
        const remaining=capture.files.filter(value=>!!this.read(value.name)).map(value=>value.name),legacyRemaining=this.legacyKeys(capture.scope).sort();
        const complete=!blocked.length&&!remaining.length&&!legacyRemaining.length;
        return {schema:1,scope:capture.scope,operation_id:operationId,complete,state:complete?'complete':'blocked',
            removed_files:capture.files.filter(value=>!remaining.includes(value.name)).map(value=>value.name),remaining_files:remaining,
            blocked,legacy_remaining:legacyRemaining,late_observed_legacy:lateObserved,fence_sha256:digest(fence.bytes)};
    }
    keys():readonly string[]{return [...this.legacy.keys().filter(key=>!key.startsWith(INTENT_PREFIX)),...this.roster().keys()];}
    get<T>(key:string):T|undefined{
        if(!key.startsWith(INTENT_PREFIX))return this.legacy.get<T>(key);
        return this.bounded(()=>{
            const {scope}=nameKey(key),legacy=this.legacy.get(key),receipt=this.importReceipt(scope),entry=this.entry(key);
            if(legacy!==undefined){
                const value=validateStoredIntent(key,legacy);
                if(this.revokedLegacy(key,value))return entry?.value as T|undefined;
                if(receipt){if(receipt.keys[key]!==intentHash(value))throw new ServiceError('private_intent_legacy_outcome_requires_review');}
                else{if(entry&&!isDeepStrictEqual(immutable(entry.value),immutable(value)))throw new ServiceError('private_intent_legacy_request_changed');if(!entry)return value as T;}
            }
            return entry?.value as T|undefined;
        });
    }
    async update(key:string,value:unknown):Promise<void>{
        if(key.startsWith(INTENT_PREFIX))throw new ServiceError('private_intent_generic_update_forbidden');
        this.authorized();await this.legacy.update(key,value);this.authorized();
    }
    private createBase(key:string,input:unknown,published?:(value:Publication)=>void):void{
        const value=validateStoredIntent(key,mergeAcceptedIntentEvidence(validateStoredIntent(key,normalized(input)),[])),{scope,stem}=nameKey(key);
        const current=this.entry(key);if(current){if(!isDeepStrictEqual(immutable(current.value),immutable(value)))throw new ServiceError('private_intent_request_changed');return;}
        const base:Base={schema:1,key,generation:randomUUID(),value};
        if(Buffer.byteLength(json(base))>MAX_FILE_BYTES)throw new ServiceError('private_intent_record_capacity');
        const name='b-'+stem+'.json';
        this.exclusive(name,base,{scope,beforePublish:()=>{if(this.read('r-'+scope+'.json'))throw new ServiceError('private_intent_scope_retired');if(this.read(this.retirementName(key)))throw new ServiceError('private_intent_retirement_in_progress');},
            published:physical=>published?.({name,hash:digest(json(base)),stamp:physical})});
    }
    async create(key:string,input:unknown):Promise<void>{
        this.guard();const {scope}=nameKey(key);if(!this.scopes.has(scope))throw new ServiceError('private_intent_scope_not_initialized');
        if(this.read('r-'+scope+'.json'))throw new ServiceError('private_intent_scope_retired');
        if(this.read(this.retirementName(key)))throw new ServiceError('private_intent_retirement_in_progress');
        const before=this.roster(),value=validateStoredIntent(key,mergeAcceptedIntentEvidence(validateStoredIntent(key,normalized(input)),[]));
        if(!before.has(key)&&(before.size>=INTENT_LIMIT||this.charged(before)+chargedRecordBytes(value)>INTENT_MAX_TOTAL_BYTES))throw new ServiceError('private_intent_global_capacity');
        let publication:Publication|undefined;const anchors:Publication[]=[];
        try{
            this.createBase(key,value,own=>{publication=own;});if(this.read('r-'+scope+'.json'))throw new ServiceError('private_intent_scope_retired');
            if(this.read(this.retirementName(key)))throw new ServiceError('private_intent_retirement_in_progress');
            const prepared=this.entry(key);if(!prepared)throw new ServiceError('private_intent_create_unconfirmed');
            this.publishAnchors(key,prepared,value,anchors);
            const after=this.roster();if(after.size>INTENT_LIMIT||this.charged(after)>INTENT_MAX_TOTAL_BYTES)throw new ServiceError('private_intent_global_capacity');
            const current=this.entry(key);
            if(!current||!isDeepStrictEqual(immutable(current.value),immutable(value))
                ||publication&&(current.hash!==publication.hash||!sameStamp(current.stamp,publication.stamp)))throw new ServiceError('private_intent_create_unconfirmed');
        }catch(error){
            if(publication){
                this.rollbackAnchors(key,publication,anchors);
                const current=this.read(publication.name),overlay=this.read('o-'+nameKey(key).stem+'.json');
                // No other writer's generation or observation is discarded.
                // This call never dispatched native work before returning ACK.
                const anyAnchor=this.names().some(name=>name.startsWith('a-'+nameKey(key).stem+'-'));
                if(current&&!overlay&&!anyAnchor&&selectAcceptedIntentEvidence(value).length===0&&digest(current.bytes)===publication.hash&&sameStamp(current.stamp,publication.stamp)){
                    this.guard();fs.unlinkSync(this.file(publication.name));this.syncDirectory();
                }
            }
            throw error;
        }
    }
    async replace(key:string,input:unknown):Promise<void>{
        this.roster();let value=validateStoredIntent(key,normalized(input));const before=this.entry(key),{scope,stem}=nameKey(key);
        if(!this.scopes.has(scope)||!before)throw new ServiceError('private_intent_not_found');
        if(this.read('r-'+scope+'.json'))throw new ServiceError('private_intent_scope_retired');
        if(this.read(this.retirementName(key)))throw new ServiceError('private_intent_retirement_in_progress');
        if(!isDeepStrictEqual(immutable(before.base.value),immutable(value)))throw new ServiceError('private_intent_request_changed');
        const ownedAnchors:Publication[]=[];
        try{value=this.publishAnchors(key,before,value,ownedAnchors);}catch(error){this.rollbackAnchors(key,before,ownedAnchors);throw error;}
        const source=before.base.value as unknown as Record<string,unknown>,target=withoutAcceptedIntentEvidence(value) as unknown as Record<string,unknown>;
        const changes=Object.fromEntries(Object.entries(target).filter(([field,item])=>!isDeepStrictEqual(item,source[field])));
        const observation={schema:1,key,base_sha256:before.hash,changes,removed:Object.keys(source).filter(field=>!(field in target))};
        const bytes=Buffer.byteLength(json(value));
        const roster=this.roster(),other=this.charged(roster,key);
        if(roster.size>INTENT_LIMIT||other+Math.max(bytes,chargedRecordBytes(value))>INTENT_MAX_TOTAL_BYTES)throw new ServiceError('private_intent_global_capacity');
        const temporary=this.temporary(observation,scope);
        const publication={hash:digest(json(observation)),stamp:this.publications.get(temporary)!};let published=false;
        try{
            const current=this.entry(key);if(!current||current.hash!==before.hash||!sameStamp(current.stamp,before.stamp))throw new ServiceError('private_intent_base_changed');
            if(!isDeepStrictEqual(current.overlay,before.overlay))throw new ServiceError('private_intent_observation_changed');
            if(this.read('r-'+scope+'.json'))throw new ServiceError('private_intent_scope_retired');
            if(this.read(this.retirementName(key)))throw new ServiceError('private_intent_retirement_in_progress');
            value=this.projectedCapacity(key,value,current,this.anchors(key,before.base,before.hash).map(item=>item.evidence));
            this.publicationFence(key,before,temporary);
            this.guard();fs.renameSync(this.file(temporary),this.file('o-'+stem+'.json'));published=true;this.publications.delete(temporary);this.syncDirectory();
            const after=this.entry(key);if(!after||after.hash!==before.hash||!sameStamp(after.stamp,before.stamp))throw new ServiceError('private_intent_base_changed');
            const expected=mergeAcceptedIntentEvidence(value,this.anchors(key,before.base,before.hash).map(item=>item.evidence));
            if(!isDeepStrictEqual(after.value,expected))throw new ServiceError('private_intent_observation_unconfirmed');
            if(this.read('r-'+scope+'.json'))throw new ServiceError('private_intent_scope_retired');
            if(this.read(this.retirementName(key)))throw new ServiceError('private_intent_retirement_in_progress');
            this.roster();
        }catch(error){
            if(published){
                const current=this.entry(key);
                const fencedUnobserved=before.overlay===undefined&&error instanceof ServiceError&&['private_intent_scope_retired','private_intent_retirement_in_progress'].includes(error.code);
                if(!current||current.hash!==before.hash||!sameStamp(current.stamp,before.stamp)||fencedUnobserved){
                    const own=this.read('o-'+stem+'.json');
                    if(own&&digest(own.bytes)===publication.hash&&sameStamp(own.stamp,publication.stamp)){
                        this.guard();fs.unlinkSync(this.file('o-'+stem+'.json'));this.syncDirectory();
                    }
                }
            }
            this.rollbackAnchors(key,before,ownedAnchors);
            throw error;
        }finally{this.removeTemporary(temporary);}
    }
    async retire(key:string):Promise<void>{
        this.guard();const {scope,stem}=nameKey(key),name=this.retirementName(key);
        if(!this.scopes.has(scope))throw new ServiceError('private_intent_scope_not_initialized');
        let receipt=this.read(name),proof:RetirementProof;
        if(receipt)proof=this.proof(receipt.value,name);
        else{
            this.roster();const before=this.entry(key);if(!before)return;
            if(this.read('r-'+scope+'.json'))throw new ServiceError('private_intent_scope_retired');
            const files:CapturedFile[]=[{name:'b-'+stem+'.json',sha256:before.hash,
                physical:{dev:String(before.stamp.dev),ino:String(before.stamp.ino),born:String(before.stamp.born)}}];
            if(before.overlay)files.push({name:'o-'+stem+'.json',sha256:before.overlay.hash,
                physical:{dev:String(before.overlay.stamp.dev),ino:String(before.overlay.stamp.ino),born:String(before.overlay.stamp.born)}});
            files.push(...before.anchors);
            proof={schema:1,kind:'retire_exact_private_intent',key,directory:this.directory,root:this.rootBinding(),generation:before.base.generation,files};
            this.proof(proof,name);
            try{this.exclusive(name,proof,{scope,beforePublish:()=>{if(this.read('r-'+scope+'.json'))throw new ServiceError('private_intent_scope_retired');}});}
            catch(error){if((error as NodeJS.ErrnoException).code!=='EEXIST'||!isDeepStrictEqual(this.read(name)?.value,proof))throw error;}
            receipt=this.read(name);if(!receipt||!isDeepStrictEqual(this.proof(receipt.value,name),proof))throw new ServiceError('private_intent_retirement_proof_changed');
        }
        // The durable exact proof fences observations before the authority base
        // disappears. Its private cleanup can resume without any native replay.
        // One asynchronous API boundary lets an already-running same-process
        // observer finish its mandatory retirement-fence check. A paused
        // cross-process producer leaves an explicit bounded proof, never a wait
        // loop or a claim that cleanup completed.
        await Promise.resolve();
        this.bounded(()=>this.settleRetirement(proof));
    }
}
