# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/exact-scope/source/packages/vscode/src')
f=p/'serviceBootstrap.ts';s=f.read_text(encoding='utf-8');pos=s.index('function projectRegistration(')
s=s[:pos]+'''export interface NativeSourceScope {
    readonly root:string; readonly collection:string|null;
    readonly namespace:string|null; readonly revision:number|null;
    readonly observed_unix_ms:number|null;
    readonly outcome:'registered'|'missing'|'removed'|'ambiguous';
    readonly registration:Readonly<{binding:string;root:string;data_root:string;collection:string}>|null;
    readonly fencing_generation:number|null;
}
const exactScopeOwners=new WeakMap<NativeSourceScope,{binding:string;root:string;collection:string|null}>();
const exactBindingIdentity=(binding:RuntimeBinding)=>JSON.stringify(binding);
export function exactScopeMatches(value:NativeSourceScope,binding:RuntimeBinding,root:string,collection:string|null):boolean {
    const owner=exactScopeOwners.get(value);
    return !!owner&&owner.binding===exactBindingIdentity(binding)&&owner.root===root&&owner.collection===collection;
}
''' +s[pos:]
needle='    client(binding:RuntimeBinding):ServiceClient {'
method='''    /** Native saved-scope observation only; not permission to start or broker evidence. */
    async sourceScope(binding:RuntimeBinding,root:string,collection:string|null):Promise<NativeSourceScope> {
        this.active();safePath(root);
        if(binding.version!==this.version||binding.payloadVerified===false)throw new ServiceError('selected_runtime_payload_unverified');
        if(collection!==null&&(!/^[A-Za-z0-9._-]{1,128}$/.test(collection)||['.','..'].includes(collection)))throw new ServiceError('invalid_project_registration');
        const identity=exactBindingIdentity(binding);
        const response=await this.invoke(binding.executable,['runtime','source-scope','--install-dir',binding.installDir,
            '--expected-control',binding.controlDir,'--expected-runtime',binding.runtimeSha256,'--root',root,
            ...(collection===null?[]:['--collection',collection])],15000);
        this.active();
        if(identity!==exactBindingIdentity(binding)||response.install_dir!==binding.installDir||response.control_dir!==binding.controlDir||response.runtime_sha256!==binding.runtimeSha256)throw new ServiceError('crossed_source_scope_observation');
        const raw=record(response.scope);
        if(Object.keys(raw).sort().join(',')!=='collection,fencing_generation,namespace,observed_unix_ms,outcome,registration,revision,root,schema'||raw.schema!==1||raw.collection!==collection||!['registered','missing','removed','ambiguous'].includes(String(raw.outcome)))throw new ServiceError('invalid_source_scope_observation');
        const canonical=safePath(raw.root);
        if(path.relative(canonical,root)!=='')throw new ServiceError('crossed_source_scope_observation');
        const namespace=raw.namespace===null?null:identifier(raw.namespace,32);
        const revision=raw.revision===null?null:(count(raw.revision),raw.revision as number);
        const observed=raw.observed_unix_ms===null?null:(count(raw.observed_unix_ms),raw.observed_unix_ms as number);
        if((namespace===null)!==(revision===null)||namespace===null&&raw.outcome!=='missing')throw new ServiceError('invalid_source_scope_observation');
        const registration=raw.registration===null?null:projectRegistration(raw.registration);
        const generation=raw.fencing_generation===null?null:(count(raw.fencing_generation),raw.fencing_generation as number);
        if(raw.outcome==='registered'){
            if(!registration||namespace===null||generation===null||path.relative(registration.root,canonical)!==''||collection!==null&&registration.collection!==collection)throw new ServiceError('crossed_source_scope_observation');
        }else if(registration!==null||generation!==null)throw new ServiceError('invalid_source_scope_observation');
        if(registration)Object.freeze(registration);
        const value:NativeSourceScope=Object.freeze({root:canonical,collection,namespace,revision,observed_unix_ms:observed,outcome:raw.outcome as NativeSourceScope['outcome'],registration,fencing_generation:generation});
        exactScopeOwners.set(value,{binding:identity,root,collection});return value;
    }
'''
assert needle in s;s=s.replace(needle,method+needle);f.write_text(s,encoding='utf-8',newline='\n')
f=p/'serviceWorkspace.ts';s=f.read_text(encoding='utf-8');s=s.replace('ProjectRegistrationReview, RuntimeBinding, RuntimeHost','exactScopeMatches, NativeSourceScope, ProjectRegistrationReview, RuntimeBinding, RuntimeHost')
s=s.replace("{kind:'registered';spec:LaunchSpec;revision:number}\n    |{kind:'removed'|'missing';revision:number};", "{kind:'registered';spec:LaunchSpec;revision:number;namespace:string;fencing_generation:number}\n    |{kind:'removed'|'missing'|'ambiguous';revision:number|null;namespace:string|null};")
start=s.index('    async selectedObservation(');end=s.index('    /** Trusted startup',start)
s=s[:start]+'''    async selectedObservation(binding:RuntimeBinding,opened:string,
        read:(root:string,collection:string|null)=>Promise<NativeSourceScope>):Promise<SelectedRootObservation>{
        this.active();
        const saved=(await this.definitions(binding,[opened]))[0];this.active();
        let root:string;
        try{root=await fs.realpath(opened);}catch(error){
            if(!saved||!['ENOENT','ENOTDIR','EACCES','EPERM'].includes((error as NodeJS.ErrnoException).code??''))throw error;
            root=saved.root;
        }
        const collection=saved?.collection??null;
        const observed=await read(root,collection);this.active();
        if(!exactScopeMatches(observed,binding,root,collection))throw new ServiceError('workspace_observation_not_authoritative');
        if(observed.outcome!=='registered')return {kind:observed.outcome,revision:observed.revision,namespace:observed.namespace};
        const project=observed.registration!;
        if(!binding.launcher)throw new ServiceError('stable_launcher_unavailable');
        return {kind:'registered',revision:observed.revision!,namespace:observed.namespace!,fencing_generation:observed.fencing_generation!,
            spec:{command:binding.universalLauncher??binding.launcher,root:project.root,dataDir:project.data_root,collection:project.collection,name:path.basename(opened),
            service:{installDir:binding.installDir,project:project.binding,...(binding.universalLauncher?{universal:true}:{})}}};
    }
'''+s[end:];f.write_text(s,encoding='utf-8',newline='\n')
f=p/'serviceExtension.ts';s=f.read_text(encoding='utf-8');s=s.replace("private async observeSelected(binding:RuntimeBinding):Promise<Awaited<ReturnType<typeof readWorkspaceRegistrations>>>","private async observeSelected(binding:RuntimeBinding):Promise<void>")
start=s.index('        const client=host.client(binding);',s.index('    private async observeSelected('));end=s.index('        const active=vscode.window.activeTextEditor',start)
s=s[:start]+'        valid();\n'+s[end:]
s=s.replace('const selected=folder&&await this.workspace.selectedObservation(binding,folder.uri.fsPath,observed);valid();', 'const selected=folder&&await this.workspace.selectedObservation(binding,folder.uri.fsPath,async(root,collection)=>{const observed=await host.sourceScope(binding,root,collection);valid();return observed;});valid();')
s=s.replace('        return observed;\n    }\n    async refresh', '    }\n    async refresh',1)
needle='        const generation=this.generation,observed=await this.observeSelected(binding);this.trusted();'
replacement='''        const generation=this.generation;await this.observeSelected(binding);this.trusted();
        // Full workspace MCP definitions still require the complete console snapshot.
        // This work starts only after the exact selected UI island is published.
        const host=this.runtime(),client=host.client(binding);
        let observed:Awaited<ReturnType<typeof readWorkspaceRegistrations>>;
        try{observed=await readWorkspaceRegistrations(binding,async(after,revision,inactive)=>{
            const page=await client.projects(after,revision,32,inactive);this.trusted();
            if(generation!==this.generation||host!==this.host)throw new ServiceError('workspace_registration_changed');return page;
        });}finally{client.dispose();}
'''
assert needle in s;s=s.replace(needle,replacement);f.write_text(s,encoding='utf-8',newline='\n')
