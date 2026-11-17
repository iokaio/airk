# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
p=Path('artifacts/optimizations/f09-draft/exact-scope')
record=json.loads((p/'BASE.json').read_text(encoding='utf-8'))
for name in ['serviceWorkspace.test.ts','serviceExtension.test.ts']:
 rel='packages/vscode/src/test/'+name;old=Path('artifacts/optimizations/f09-draft/source')/rel
 record[rel]={'sha256':hashlib.sha256(old.read_bytes()).hexdigest(),'layer':old.as_posix()}
 for layer in ['base','source']:
  dst=p/layer/rel;dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(old.read_bytes())
(p/'BASE.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
f=p/'source/packages/vscode/src/test/serviceWorkspace.test.ts';s=f.read_text(encoding='utf-8').replace('ProjectRegistrationReview, RuntimeBinding','RuntimeHost, ProjectRegistrationReview, RuntimeBinding')
start=s.index("test('early selected UI spec requires")
s=s[:start]+'''test('early selected scope uses native namespace and refuses forged, removed, ambiguous and canceled observations',async()=>{
    const root=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-selected-exact-'))),source=path.join(root,'source');await fs.mkdir(source);
    const binding:RuntimeBinding={installDir:root,controlDir:path.join(root,'control'),runtimeSha256:'ab'.repeat(32),version:'1.8.0',executable:path.join(root,'airk'),launcher:path.join(root,'launcher'),lifecycle:'on-demand',verificationBasis:'trusted_distribution'};
    let allowed=true,namespace='11'.repeat(16),revision=9,outcome='registered',project='cd'.repeat(32);
    const registration={root:source,data_root:path.join(root,'data'),collection:'fixture',binding:project};
    const values=new Map<string,unknown>();
    const workspace=new ServiceWorkspace({keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key,value)=>{values.set(key,value);}},
        {reviewProject:async()=>({...binding,...registration}),registerProject:async(_binding,value)=>value},()=>allowed);
    let suspend:Promise<void>|undefined;const requests:string[][]=[];
    const host=new RuntimeHost(root,'1.8.0',{channel:'stable'},async(command,args)=>{
        assert.equal(command,binding.executable);assert.deepEqual(args.slice(0,2),['runtime','source-scope']);requests.push(args);
        if(suspend)await suspend;
        return {install_dir:binding.installDir,control_dir:binding.controlDir,runtime_sha256:binding.runtimeSha256,
            scope:{schema:1,root:source,collection:args.includes('--collection')?'fixture':null,namespace,revision,observed_unix_ms:17,outcome,
                registration:outcome==='registered'?{...registration,binding:project}:null,fencing_generation:outcome==='registered'?3:null}};
    },()=>allowed);
    const read=(root:string,collection:string|null)=>host.sourceScope(binding,root,collection);
    try{
        await assert.rejects(workspace.selectedObservation(binding,source,async()=>({} as any)),/not_authoritative/);
        const first=await workspace.selectedObservation(binding,source,read);
        assert.equal(first.kind,'registered');assert.equal(first.namespace,namespace);
        if(first.kind==='registered'){assert.equal(first.fencing_generation,3);assert.equal(first.spec.service!.project,project);}
        const observed=await read(source,null);assert.ok(Object.isFrozen(observed));assert.ok(Object.isFrozen(observed.registration));
        await assert.rejects(workspace.selectedObservation({...binding,controlDir:path.join(root,'other')},source,async()=>observed),/not_authoritative/);
        await workspace.register(binding,source,registration.data_root,registration.collection,async()=>true);
        for(const state of ['removed','missing','ambiguous']){outcome=state;assert.equal((await workspace.selectedObservation(binding,source,read)).kind,state);}
        outcome='registered';namespace='22'.repeat(16);project='ef'.repeat(32);revision=10;
        const replaced=await workspace.selectedObservation(binding,source,read);
        assert.equal(replaced.namespace,namespace);if(replaced.kind==='registered')assert.equal(replaced.spec.service!.project,project);
        let release!:()=>void;suspend=new Promise<void>(resolve=>{release=resolve;});
        const pending=workspace.selectedObservation(binding,source,read);await new Promise<void>(resolve=>setImmediate(resolve));allowed=false;release();
        await assert.rejects(pending,/no_longer_authorized/);
        assert.ok(requests.every(args=>!args.includes('console')&&!args.includes('ensure')));
    }finally{await fs.rm(root,{recursive:true,force:true});}
});
''';f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/packages/vscode/src/test/serviceExtension.test.ts';s=f.read_text(encoding='utf-8')
# Exercise production sourceScope parser+brand using its actual invoke seam, not forged observation objects.
needle="    context.mock.method(RuntimeHost.prototype,'client',"
positions=[];start=0
while True:
 i=s.find(needle,start)
 if i<0:break
 positions.append(i);start=i+len(needle)
for index,i in reversed(list(enumerate(positions))):
 row='registration' if index==0 else 'registrations.find(value=>value.root===args[args.indexOf(\'--root\')+1])!'
 insertion="""    context.mock.method(RuntimeHost.prototype as any,'invoke',async(_command:string,args:string[])=>{
        assert.deepEqual(args.slice(0,2),['runtime','source-scope']);
        const row=ROW;
        return {install_dir:binding.installDir,control_dir:binding.controlDir,runtime_sha256:binding.runtimeSha256,
            scope:{schema:1,root:row.root,collection:args.includes('--collection')?args[args.indexOf('--collection')+1]:null,
                namespace:'12'.repeat(16),revision:7,observed_unix_ms:1,outcome:'registered',registration:row,fencing_generation:1}};
    });
""".replace('ROW',row)
 s=s[:i]+insertion+s[i:]
f.write_text(s,encoding='utf-8',newline='\n')
