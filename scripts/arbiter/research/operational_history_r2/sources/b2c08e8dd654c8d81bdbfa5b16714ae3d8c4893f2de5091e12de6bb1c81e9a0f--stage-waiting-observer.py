# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path(__file__).resolve().parents[2]
controller_name='packages/vscode/src/explorerController.ts'
test_name='packages/vscode/src/test/explorerShell.test.ts'
controller=(root/controller_name).read_text(encoding='utf-8')
tests=(root/test_name).read_text(encoding='utf-8')
new_controller=controller.replace('catalogExport, CatalogObject, WindowContextDescriptor', 'catalogExport, CatalogObject, WindowContextDescriptor, validateNativeHostContext, validateWindowContext')
assert new_controller!=controller
before='    private clearContextObservationTimer(): void {'
after="""    private completeWindowContextObservation():boolean {
        try{
            const descriptor=validateWindowContext(this.host.windowContext?.()),observation=this.host.windowContextObservation?.();
            if(observation?.status!=='catalog_host_context')return false;
            const native=validateNativeHostContext(observation.descriptor),capabilities=observation.capabilities;
            if(!capabilities||typeof capabilities!=='object'||Array.isArray(capabilities))return false;
            const contract=(capabilities as CatalogObject).policy_contract;
            return native.host===descriptor.host&&native.key===descriptor.key&&typeof contract==='number'&&Number.isSafeInteger(contract)&&contract>=0;
        }catch{return false;}
    }
    private clearContextObservationTimer(): void {"""
assert new_controller.count(before)==1
new_controller=new_controller.replace(before,after)
before="""        let observation = this.contextObservation;
        if (retry"""
after="""        let observation = this.contextObservation;
        // An explicit read can finish while the automatic observer is backing
        // off. Its current authenticated result satisfies the same owner; do
        // not later reread a newer catalog clock and retire an active review.
        if(!this.contextObservationRead&&this.completeWindowContextObservation()){
            this.clearContextObservationTimer();observation.state='complete';return;
        }
        if (retry"""
assert new_controller.count(before)==1
new_controller=new_controller.replace(before,after)

added_tests=r'''
function completeAutomaticWindow(f:ReturnType<typeof automaticContextFixture>){
    f.host.windowContext=()=>{const native=f.host.windowContextObservation()?.descriptor;return native?{host:native.host,key:native.key,kind:'folder',name:'Fixture'}:undefined;};
}
test('waiting automatic observation is satisfied by explicit page success without a later clock-catching read',async t=>{
    t.mock.timers.enable({apis:['setTimeout','Date']});
    const f=automaticContextFixture();completeAutomaticWindow(f);let reads=0,nativeRevision=7;
    f.host.prepareWindowContext=async()=>{
        reads++;if(reads===1)throw new ServiceError('catalog_control_connection_unavailable');
        f.observe(f.generation());(f.host.windowContextObservation().descriptor as any).catalog_revision=nativeRevision;
        f.instance.observeConnectedContext(); // actual producer's context-change event
    };
    f.host.control.controller.catalogRead=async()=>({status:'catalog_page',page:{namespace:f.generation().toString(16).padStart(32,'0'),revision:nativeRevision,items:[],continuation:null},capabilities:{policy_contract:3}});
    try{
        f.explorer.open();await contextMicrotasks();assert.equal(reads,1);assert.equal(f.instance.contextObservation.state,'waiting');
        await f.instance.dispatch({type:'indexing.page',kind:'indexes'});assert.equal(reads,2);assert.ok(f.explorer.snapshot().indexing?.page);
        // Native setup can advance the catalog after this explicit page. A
        // leftover retry must not introduce that later clock into its owner.
        nativeRevision=9;t.mock.timers.tick(250);await contextMicrotasks();
        const current=f.explorer.snapshot().indexing;
        assert.deepEqual({reads,page:!!current?.page,revision:current?.revision},{reads:2,page:true,revision:7});
        t.mock.timers.tick(30000);await contextMicrotasks();assert.equal(reads,2);assert.deepEqual(f.runs,[]);
    }finally{f.explorer.dispose();t.mock.timers.reset();}
});

test('already complete current native owner does not start an automatic capability read',async()=>{
    const f=automaticContextFixture();completeAutomaticWindow(f);let reads=0;
    f.observe(f.generation());f.host.prepareWindowContext=async()=>{reads++;throw new Error('complete owner must not refetch');};
    try{f.instance.observedControl();await contextMicrotasks();assert.equal(reads,0);assert.equal(f.explorer.snapshot().indexing?.canReviewSplit,true);}
    finally{f.explorer.dispose();}
});

test('retired window and control observations cannot satisfy a replacement native owner',async t=>{
    t.mock.timers.enable({apis:['setTimeout','Date']});
    try{
        for(const replacement of ['window','control'] as const){
            const f=automaticContextFixture();let reads=0,release!:()=>void,payload:any;
            const held=new Promise<void>(resolve=>{release=resolve;}),owner=()=>JSON.stringify([f.generation(),f.host.control.controller.observationIdentity]);
            f.host.windowContextObservation=()=>payload?.owner===owner()?payload.response:undefined;completeAutomaticWindow(f);
            f.host.prepareWindowContext=async()=>{
                reads++;const producer=owner();if(reads===1)throw new ServiceError('registry_busy');if(reads===2)await held;
                payload={owner:producer,response:{status:'catalog_host_context',capabilities:{policy_contract:3},descriptor:{schema:1,namespace:f.generation().toString(16).padStart(32,'0'),host:'22'.repeat(32),key:'33'.repeat(32),catalog_revision:1,context:null}}};
                f.instance.observeConnectedContext();
            };
            try{
                f.explorer.open();await contextMicrotasks();t.mock.timers.tick(250);await contextMicrotasks();assert.equal(reads,2);
                if(replacement==='window')f.nextWindow();else{f.host.control.controller.observationIdentity='replacement';f.instance.observedControl();}
                assert.equal(f.explorer.snapshot().indexing?.canReviewSplit,false);release();await contextMicrotasks();
                assert.equal(reads,3,replacement);assert.equal(payload.owner,owner());assert.equal(f.explorer.snapshot().indexing?.canReviewSplit,true);
                t.mock.timers.tick(4000);await contextMicrotasks();assert.equal(reads,3);assert.deepEqual(f.runs,[]);
            }finally{release();f.explorer.dispose();}
        }
    }finally{t.mock.timers.reset();}
});

test('external missing malformed mismatched or unknown metadata cannot satisfy a waiting observer',async t=>{
    t.mock.timers.enable({apis:['setTimeout','Date']});
    try{
        for(const malformed of ['missing','descriptor','mismatch','capability-missing','capability-array','capability-negative','capability-fraction'] as const){
            const f=automaticContextFixture();completeAutomaticWindow(f);let reads=0;
            f.host.prepareWindowContext=async()=>{reads++;throw new ServiceError('registry_busy');};
            try{
                f.explorer.open();await contextMicrotasks();assert.equal(reads,1);
                if(malformed!=='missing'){
                    f.observe(f.generation());const observation=f.host.windowContextObservation();
                    if(malformed==='descriptor')observation.descriptor.schema=99;
                    else if(malformed==='mismatch')f.host.windowContext=()=>({host:'44'.repeat(32),key:'33'.repeat(32),kind:'folder',name:'Fixture'});
                    else if(malformed==='capability-missing')delete observation.capabilities;
                    else if(malformed==='capability-array')observation.capabilities=[];
                    else if(malformed==='capability-negative')observation.capabilities.policy_contract=-1;
                    else observation.capabilities.policy_contract=1.5;
                }
                f.instance.observeConnectedContext();assert.equal(f.instance.contextObservation.state,'waiting',malformed);
                t.mock.timers.tick(250);await contextMicrotasks();assert.equal(reads,2,malformed);assert.deepEqual(f.runs,[]);
            }finally{f.explorer.dispose();}
        }
    }finally{t.mock.timers.reset();}
});
'''
before="test('an open Explorer follows the replacement native catalog controller after automatic upgrade',async()=>{"
assert tests.count(before)==1
new_tests=tests.replace(before,added_tests+'\n'+before)
stage=root/'artifacts/qa-1.8.9/waiting-observer-staged'
for name,value in [(controller_name,new_controller),(test_name,new_tests)]:
    target=stage/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(value,encoding='utf-8',newline='\n')
patch=''.join(difflib.unified_diff(controller.splitlines(True),new_controller.splitlines(True),fromfile='a/'+controller_name,tofile='b/'+controller_name))
patch+=''.join(difflib.unified_diff(tests.splitlines(True),new_tests.splitlines(True),fromfile='a/'+test_name,tofile='b/'+test_name))
target=root/'artifacts/qa-1.8.9/waiting-observer.patch';target.write_text(patch,encoding='utf-8',newline='\n');print(target)
