// SPDX-License-Identifier: Apache-2.0
//! Reviewed workspace bindings survive extension replacement; indexes belong to the service.
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { LaunchSpec } from './model';
import { ProjectRegistrationReview, RuntimeBinding, RuntimeHost } from './serviceBootstrap';
import { identifier, record, ServiceError, type ConsolePage } from './serviceClient';
import { IntentStorage } from './serviceIntents';

const PREFIX='airk.service.workspace.1.';
const ACTIVE='airk.service.workspace.active.1.';
interface SavedProject {schema:1;opened:string;registration:ProjectRegistrationReview}
export interface WorkspaceRegistrations { active:ProjectRegistrationReview[]; inactive:ProjectRegistrationReview[] }
function absolute(value:unknown):string {
    if(typeof value!=='string'||value.length>4096||!path.isAbsolute(value)||/[\u0000-\u001f\u007f]/.test(value))throw new ServiceError('invalid_saved_workspace_path');
    return value;
}
function same(left:string,right:string):boolean {
    return path.relative(process.platform==='win32'?path.toNamespacedPath(left):left,process.platform==='win32'?path.toNamespacedPath(right):right)==='';
}
/** Follow the native default after a rename without redirecting other custom storage. */
export async function renamedDefaultDataRoot(configured:string,nativeData:string):Promise<string|undefined>{
    absolute(configured);absolute(nativeData);
    const leaf=path.basename(nativeData);
    if(!['index-v1','index-v1-preview'].includes(leaf))return undefined;
    const previous=path.join(path.dirname(nativeData),leaf==='index-v1'?'service-1.4':'service-1.4-preview');
    if(!same(configured,previous))return undefined;
    try{await fs.lstat(previous);return undefined;}
    catch(error){if(['ENOENT','ENOTDIR'].includes((error as NodeJS.ErrnoException).code??''))return nativeData;throw error;}
}
/** Complete, revision-consistent native observations; a missing page is never an empty registry. */
export async function readWorkspaceRegistrations(binding:RuntimeBinding,
    read:(after:string|null,revision:number|null,inactive:boolean)=>Promise<ConsolePage>):Promise<WorkspaceRegistrations>{
    const result:WorkspaceRegistrations={active:[],inactive:[]},seen=new Set<string>();let revision:number|null=null;
    for(const inactive of [false,true]){
        let after:string|null=null;
        do{
            const page=await read(after,revision,inactive);
            if(revision!==null&&page.revision!==revision)throw new ServiceError('workspace_registry_changed');revision=page.revision;
            for(const row of page.projects){
                const project=record(record(row.project).registration),id=identifier(project.binding,32);
                if(seen.has(id)||seen.size>=1024)throw new ServiceError('workspace_registry_page_invalid');seen.add(id);
                const collection=project.collection;
                if(typeof collection!=='string'||!/^[A-Za-z0-9._-]{1,128}$/.test(collection)||['.','..'].includes(collection))throw new ServiceError('invalid_saved_workspace_registration');
                result[inactive?'inactive':'active'].push({installDir:binding.installDir,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256,
                    binding:id,root:absolute(project.root),data_root:absolute(project.data_root),collection});
            }
            if(page.after!==null&&(page.after===after||!page.projects.length||seen.size>=1024))throw new ServiceError('workspace_registry_page_invalid');
            after=page.after;
        }while(after!==null);
    }
    return result;
}
/** Workspace state remembers reviewed/native bindings; sources remain read-only. */
export class ServiceWorkspace {
    constructor(private readonly storage:IntentStorage,private readonly runtime:Pick<RuntimeHost,'reviewProject'|'registerProject'>,
        private readonly authorized:()=>boolean){}
    private active():void {if(!this.authorized())throw new ServiceError('runtime_client_no_longer_authorized');}
    private selection(install:string,opened:string):string{return ACTIVE+createHash('sha256').update(JSON.stringify([install,opened])).digest('hex');}
    hasRegistration(binding:RuntimeBinding,opened:string):boolean{
        this.active();
        return this.saved().some(value=>same(value.opened,opened)&&same(value.registration.installDir,binding.installDir)
            &&same(value.registration.controlDir,binding.controlDir));
    }
    /** Trusted startup may recreate a lost registration; native removal remains authoritative. */
    async reconcileStartup(binding:RuntimeBinding,opened:string,observed:WorkspaceRegistrations):Promise<'registered'|'removed'|'missing'>{
        this.active();const saved=(await this.definitions(binding,[opened]))[0];
        let root:string;
        try{root=await fs.realpath(opened);}catch(error){if(!saved)throw error;root=saved.root;}
        this.active();
        const candidates=observed.active.filter(project=>same(project.root,root));
        const exact=saved&&candidates.find(project=>project.binding===saved.service!.project);
        const matching=saved?candidates.filter(project=>project.collection===saved.collection):candidates;
        const preferred=matching.length===1?matching[0]:undefined;
        if(!exact&&!preferred&&candidates.length>1)throw new ServiceError('workspace_binding_review_required');
        const project=exact||preferred||candidates[0];
        if(project){
            const key=PREFIX+createHash('sha256').update(JSON.stringify([project.installDir,project.binding,opened])).digest('hex');
            if(this.storage.get(key)===undefined&&this.saved().length>=1024)throw new ServiceError('workspace_registration_capacity');
            const existing=this.storage.get(key),current=existing?record(record(existing).registration):undefined;
            if(!current||Object.entries(project).some(([name,value])=>current[name]!==value)){
                await this.storage.update(key,{schema:1,opened,registration:project} satisfies SavedProject);this.active();
            }
            if(this.storage.get(this.selection(project.installDir,opened))!==project.binding){
                await this.storage.update(this.selection(project.installDir,opened),project.binding);this.active();
            }
            return 'registered';
        }
        if(observed.inactive.some(project=>same(project.root,root)&&(!saved||project.collection===saved.collection))){
            await this.withdraw(binding,opened);
            return 'removed';
        }
        return 'missing';
    }
    /** Remove editor launch details after authoritative native removal. */
    async withdraw(binding:RuntimeBinding,opened:string):Promise<void>{
        this.active();
        for(const key of this.storage.keys().filter(key=>key.startsWith(PREFIX))){
            const value=record(this.storage.get(key)),registration=record(value.registration);
            if(same(absolute(value.opened),opened)&&same(absolute(registration.installDir),binding.installDir)){
                await this.storage.update(key,undefined);this.active();
            }
        }
        await this.storage.update(this.selection(binding.installDir,opened),undefined);this.active();
    }
    async withdrawProject(binding:RuntimeBinding,project:string):Promise<void>{
        this.active();identifier(project,32);
        for(const key of this.storage.keys().filter(key=>key.startsWith(PREFIX))){
            const value=record(this.storage.get(key)),registration=record(value.registration);
            if(registration.binding!==project||!same(absolute(registration.installDir),binding.installDir)||!same(absolute(registration.controlDir),binding.controlDir))continue;
            const opened=absolute(value.opened);await this.storage.update(key,undefined);this.active();
            const selected=this.selection(binding.installDir,opened);
            if(this.storage.get(selected)===project){await this.storage.update(selected,undefined);this.active();}
        }
    }
    private saved():SavedProject[]{
        const keys=this.storage.keys().filter(key=>key.startsWith(PREFIX)).sort();
        if(keys.length>1024)throw new ServiceError('workspace_registration_capacity');
        return keys.map(key=>{
            const value=record(this.storage.get(key)),registration=record(value.registration);
            if(value.schema!==1)throw new ServiceError('invalid_saved_workspace_registration');
            absolute(value.opened);absolute(registration.installDir);absolute(registration.controlDir);absolute(registration.root);absolute(registration.data_root);
            identifier(registration.binding,32);identifier(registration.runtimeSha256,32);
            if(typeof registration.collection!=='string'||!/^[A-Za-z0-9._-]{1,128}$/.test(registration.collection)||['.','..'].includes(registration.collection))throw new ServiceError('invalid_saved_workspace_registration');
            return value as unknown as SavedProject;
        });
    }
    async register(binding:RuntimeBinding,opened:string,dataRoot:string,collection:string,
        confirm:(review:ProjectRegistrationReview)=>Promise<boolean>,setupWatch?:boolean,onlyIfMissing=false):Promise<ProjectRegistrationReview|undefined>{
        this.active();absolute(opened);this.saved();
        const review=await this.runtime.reviewProject(binding,opened,dataRoot,collection);this.active();
        const key=PREFIX+createHash('sha256').update(JSON.stringify([review.installDir,review.binding,opened])).digest('hex');
        if(this.storage.get(key)===undefined&&this.saved().length>=1024)throw new ServiceError('workspace_registration_capacity');
        if(!await confirm(review))return;this.active();
        await this.runtime.registerProject(binding,review,setupWatch,onlyIfMissing);this.active();
        await this.storage.update(key,{schema:1,opened,registration:review} satisfies SavedProject);this.active();
        await this.storage.update(this.selection(review.installDir,opened),review.binding);this.active();
        return review;
    }
    async definitions(binding:RuntimeBinding,openedRoots:readonly string[]):Promise<LaunchSpec[]>{
        this.active();if(!binding.launcher)throw new ServiceError('stable_launcher_unavailable');
        const saved=this.saved(),specs:LaunchSpec[]=[],seen=new Set<string>();
        for(const opened of openedRoots){
            absolute(opened);
            const candidates=saved.filter(value=>same(value.opened,opened)&&same(value.registration.installDir,binding.installDir));
            const selected=this.storage.get<string>(this.selection(binding.installDir,opened));
            if(selected!==undefined)identifier(selected,32);
            if(selected===undefined&&candidates.length>1)throw new ServiceError('workspace_binding_review_required');
            for(const value of candidates.filter(value=>selected===undefined||value.registration.binding===selected)){
                const project=value.registration;
                if(!same(project.controlDir,binding.controlDir))continue;
                try{
                    if(!same(await fs.realpath(opened),project.root))continue;
                }catch(error){
                    if(!['ENOENT','ENOTDIR','EACCES','EPERM'].includes((error as NodeJS.ErrnoException).code??''))throw error;
                    // Missing sources retain their saved binding for read-only evidence.
                }
                this.active();if(seen.has(project.binding))continue;seen.add(project.binding);
                // Agents get the version-free universal launcher when it forwards to this installation.
                specs.push({command:binding.universalLauncher??binding.launcher,root:project.root,dataDir:project.data_root,collection:project.collection,
                    name:path.basename(opened),service:{installDir:binding.installDir,project:project.binding,...(binding.universalLauncher?{universal:true}:{})}});
            }
        }
        return specs;
    }

    /** Resolve an agent's selected source against native metadata, never a saved editor binding alone. */
    async resolveConnection(binding:RuntimeBinding,spec:LaunchSpec,
        read:(after:string|null,revision:number|null)=>Promise<ConsolePage>):Promise<LaunchSpec>{
        this.active();
        if(!spec.service||!same(spec.service.installDir,binding.installDir))throw new ServiceError('workspace_registration_changed');
        const candidates:ProjectRegistrationReview[]=[],seen=new Set<string>();
        let after:string|null=null,revision:number|null=null;
        do{
            const page=await read(after,revision);this.active();
            if(revision!==null&&page.revision!==revision)throw new ServiceError('workspace_registry_changed');
            revision=page.revision;
            for(const row of page.projects){
                const project=record(record(row.project).registration),id=identifier(project.binding,32);
                if(seen.has(id)||seen.size>=1024)throw new ServiceError('workspace_registry_page_invalid');
                seen.add(id);
                const root=absolute(project.root),data_root=absolute(project.data_root);
                if(typeof project.collection!=='string'||!/^[A-Za-z0-9._-]{1,128}$/.test(project.collection)||['.','..'].includes(project.collection))throw new ServiceError('invalid_saved_workspace_registration');
                if(same(root,spec.root)&&project.collection===spec.collection)candidates.push({installDir:binding.installDir,
                    controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256,binding:id,root,data_root,collection:project.collection});
            }
            if(page.after!==null&&(page.after===after||!page.projects.length||seen.size>=1024))throw new ServiceError('workspace_registry_page_invalid');
            after=page.after;
        }while(after!==null);
        const exact=candidates.find(project=>project.binding===spec.service!.project);
        if(exact&&!same(exact.data_root,spec.dataDir))throw new ServiceError('workspace_registration_changed');
        if(!exact&&candidates.length>1)throw new Error('Several native indexes match this project. Select the intended index before connecting an agent.');
        const project=exact??candidates[0];
        if(!project)throw new Error('This project is no longer registered in the AIRK service. Add it in Management before connecting an agent.');
        const saved=this.saved().filter(value=>same(value.registration.installDir,binding.installDir)
            &&value.registration.binding===spec.service!.project&&same(value.registration.root,spec.root));
        if(!saved.length)throw new ServiceError('workspace_registration_changed');
        // Only repair the editor's reference to an already registered native index.
        // No project registration, source work, or index movement occurs here.
        for(const value of saved){
            this.active();
            if(value.registration.binding!==project.binding||!same(value.registration.data_root,project.data_root)
                ||!same(value.registration.controlDir,project.controlDir)||value.registration.runtimeSha256!==project.runtimeSha256){
                const key=PREFIX+createHash('sha256').update(JSON.stringify([project.installDir,project.binding,value.opened])).digest('hex');
                if(this.storage.get(key)===undefined&&this.saved().length>=1024)throw new ServiceError('workspace_registration_capacity');
                await this.storage.update(key,{schema:1,opened:value.opened,registration:project} satisfies SavedProject);this.active();
                await this.storage.update(this.selection(project.installDir,value.opened),project.binding);this.active();
            }
        }
        return {...spec,root:project.root,dataDir:project.data_root,collection:project.collection,
            command:binding.universalLauncher??binding.launcher??spec.command,
            service:{installDir:binding.installDir,project:project.binding,...(binding.universalLauncher?{universal:true}:{})}};
    }
}
