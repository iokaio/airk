# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

root = Path(__file__).resolve().parents[2]
changes = {}
def change(name, before, after):
    original = changes.get(name, (root / name).read_text(encoding='utf-8'))
    assert original.count(before) == 1, (name, before)
    changes[name] = original.replace(before, after)

change('packages/vscode/src/explorerController.ts',
"            case 'indexing.page': return this.indexing.page(message.kind,message.next,message.index,message.project);",
"""            case 'indexing.page': {
                this.requireAuthorization(true);
                // Establish the authenticated window owner before admitting a
                // page that can seed a native review. The producer shares its
                // in-flight read with automatic observation; this explicit read
                // has no retry loop and never selects a grant or starts work.
                if(this.host.prepareWindowContext&&(!this.host.windowContext?.()||this.host.windowContextObservation?.()?.status!=='catalog_host_context')){
                    const identity=this.control.observationIdentity,windowIdentity=this.host.windowContextIdentity?.()??this.selectionEpoch;
                    await this.host.prepareWindowContext();this.requireAuthorization(true);
                    if(identity!==this.control.observationIdentity||windowIdentity!==(this.host.windowContextIdentity?.()??this.selectionEpoch))throw new ServiceError('window_context_changed');
                    if(!this.host.windowContext?.()||this.host.windowContextObservation?.()?.status!=='catalog_host_context')throw new ServiceError('catalog_control_connection_unavailable');
                }
                return this.indexing.page(message.kind,message.next,message.index,message.project);
            }""")
change('packages/vscode/src/indexingCatalogController.ts',
"""    setupDraft(selection:CatalogObject,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            if(generation!==this.view.generation""",
"""    setupDraft(selection:CatalogObject,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            if(this.host.canReadWindowContext?.()&&(!this.descriptor()||this.host.windowContextObservation?.()?.status!=='catalog_host_context'))throw new Error('read_native_window_context_before_setup_review');
            if(generation!==this.view.generation""")

shell_tests = r'''
test('initial native catalog page shares pending automatic context and retains the established owner for reviews',async()=>{
    const f=automaticContextFixture();let requests=0,pages=0,release!:()=>void,inflight:Promise<void>|undefined;
    const held=new Promise<void>(resolve=>{release=resolve;});
    f.host.windowContext=()=>{const native=f.host.windowContextObservation()?.descriptor;return native?{host:native.host,key:native.key,kind:'folder',name:'Fixture'}:undefined;};
    f.host.prepareWindowContext=()=>inflight??= (async()=>{requests++;const generation=f.generation();await held;if(generation!==f.generation())throw new ServiceError('window_context_changed');f.observe(generation);})();
    f.host.control.controller.catalogRead=async()=>{pages++;return {status:'catalog_page',page:{namespace:'01'.padStart(32,'0'),revision:1,items:[],continuation:null},capabilities:{policy_contract:3}};};
    try{
        f.instance.observedControl();await contextMicrotasks();assert.equal(requests,1);
        const pending=f.instance.dispatch({type:'indexing.page',kind:'indexes'});await contextMicrotasks();
        assert.equal(requests,1,'explicit page shares the existing native producer request');assert.equal(pages,0,'no review-seeding page before authenticated window ownership');
        release();await pending;assert.equal(pages,1);assert.ok(f.explorer.snapshot().indexing?.windowDescriptor);
        const generation=f.explorer.snapshot().indexing?.generation;
        f.instance.observedControl();await contextMicrotasks();assert.equal(f.explorer.snapshot().indexing?.generation,generation,'late automatic completion cannot introduce another owner');
        await f.instance.dispatch({type:'indexing.page',kind:'indexes'});assert.equal(requests,1,'complete authenticated observation never refetches');assert.equal(pages,2);assert.deepEqual(f.runs,[]);
    }finally{release();f.explorer.dispose();}
});

test('initial catalog page retains typed failure and refuses changed owners without submitting a page',async()=>{
    for(const mode of ['failure','window','control','trust','native-off','missing'] as const){
        const f=automaticContextFixture();let pages=0,release!:()=>void;
        const held=new Promise<void>(resolve=>{release=resolve;}),failure=new ServiceError('registry_busy');
        f.host.windowContext=()=>f.host.windowContextObservation()?{host:'22'.repeat(32),key:'33'.repeat(32),kind:'folder',name:'Fixture'}:undefined;
        f.host.control.controller.catalogRead=async()=>{pages++;throw new Error('must not submit stale page');};
        f.host.prepareWindowContext=async()=>{await held;if(mode==='failure')throw failure;if(mode!=='missing')f.observe(f.generation());};
        try{
            const pending=f.instance.dispatch({type:'indexing.page',kind:'indexes'});await contextMicrotasks();
            if(mode==='window')f.nextWindow();else if(mode==='control')f.host.control.controller.observationIdentity='replacement';else if(mode==='trust')f.host.trusted=()=>false;else if(mode==='native-off')f.host.nativeExecutionEnabled=()=>false;
            release();await assert.rejects(pending,error=>mode==='failure'?error===failure:/window_context_changed|authorized|untrusted|native execution|native_execution|catalog_control_connection_unavailable/i.test(String(error)));
            assert.equal(pages,0,mode);assert.deepEqual(f.runs,[]);
        }finally{release();f.explorer.dispose();}
    }
});
'''
change('packages/vscode/src/test/explorerShell.test.ts',
"test('an open Explorer follows the replacement native catalog controller after automatic upgrade',async()=>{",
shell_tests+"\ntest('an open Explorer follows the replacement native catalog controller after automatic upgrade',async()=>{")

catalog_tests = r'''
test('native setup reviews require an established window owner and keep all later owner fences',async()=>{
    const f=controller(),setup=setupFixture();let descriptor:WindowContextDescriptor|undefined,observation:CatalogObject|undefined,drafts=0,applies=0;
    f.control.catalogIndexSetupDraft=async()=>{drafts++;return validateNativeIndexSetupDraft(setup.response,setup.selection);};
    f.control.catalogIndexSetupApply=async()=>{applies++;return {status:'catalog_index_setup_applied',receipt:setup.ready};};
    const value=new IndexingCatalogController({control:f.control,identity:()=> 'current',authorized:()=>true,changed(){},canReadWindowContext:()=>true,windowContext:()=>descriptor,windowContextObservation:()=>observation});
    const {key,expected_catalog_revision,...selection}=setup.selection;
    await value.page('indexes');await assert.rejects(value.setupDraft(selection,value.snapshot().generation),/read_native_window_context_before_setup_review/);assert.equal(drafts,0);assert.equal(applies,0);
    descriptor={host:'22'.repeat(32),key:'33'.repeat(32),kind:'folder',name:'Fixture'};
    observation={status:'catalog_host_context',descriptor:{schema:1,namespace,host:descriptor.host,key:descriptor.key,catalog_revision:1,context:null},capabilities:{policy_contract:3}};
    await value.adoptHostContext();await value.page('indexes');await value.setupDraft(selection,value.snapshot().generation);
    const stable=value.snapshot();assert.ok(stable.setupReview);assert.equal(value.snapshot().setupReview,stable.setupReview,'stable authenticated owner keeps exact review');
    await value.setupApply(stable.generation);assert.equal(applies,1);
    await value.page('indexes');await value.setupDraft(selection,value.snapshot().generation);const established=value.snapshot();
    observation={...observation,descriptor:{...catalogObject(observation.descriptor),catalog_revision:2}};
    assert.equal(value.snapshot().setupReview,undefined,'changed native metadata still invalidates the review');
    await assert.rejects(value.setupApply(established.generation),/stale_setup_review/);assert.equal(applies,1);assert.equal(drafts,2);
});
'''
change('packages/vscode/src/test/indexingCatalog.test.ts',
"function nativeGrantOperation(host='22'.repeat(32)){",
catalog_tests+"\nfunction nativeGrantOperation(host='22'.repeat(32)){")

output=[]
stage=root/'artifacts/qa-1.8.9/initial-catalog-owner-staged'
for name,new in changes.items():
    old=(root/name).read_text(encoding='utf-8')
    dest=stage/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text(new,encoding='utf-8',newline='\n')
    output.extend(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+name,tofile='b/'+name))
patch=root/'artifacts/qa-1.8.9/initial-catalog-owner.patch'
patch.write_text(''.join(output),encoding='utf-8',newline='\n')
print(patch)
