# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/lazy-discovery/source/packages/vscode/src/test/serviceBootstrap.test.ts')
s=p.read_text(encoding='utf-8');s+='''

function lazySignal<T>(){let resolve!:(value:T)=>void;const promise=new Promise<T>(done=>{resolve=done;});return {promise,resolve};}
async function lazyDiscoveryFixture(){
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install);
    const calls:string[][]=[];let healthCalls=0,disposed=0;
    const state={failSelfTest:false,stopped:false,healthError:undefined as Error|undefined,
        health:undefined as undefined|((number:number)=>Promise<void>)};
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:install},async(_command,args)=>{
        calls.push(args);
        switch(args[1]){
            case 'self-test':return {protocol:1,version:'1.4.0',self_test:state.failSelfTest?'failed':'passed',metadata_console:1};
            case 'location':return {protocol:1,install_dir:install};
            case 'inspect':return {selection:{active:f.digest},approval:{manifest_sha256:f.digest,version:'1.4.0',verification_basis:'trusted_distribution'},
                executable:path.join(f.source,f.main),launcher:path.join(install,'launcher'),control_dir:path.join(install,'control'),payload_verified:true,
                lifecycle:{explicitly_stopped:state.stopped,pending:null},payload_repair:{phase:'none'},registry_namespace:'12'.repeat(16),
                connection_observation:{schema:1,activation_pending:false,launcher_repair_pending:false}};
            case 'diagnose':return {fixture:'full'};
            default:throw new Error('Unexpected native execution effect');
        }
    });
    host.client=()=>new ServiceClient({async request(){
        const number=++healthCalls;if(state.health)await state.health(number);if(state.healthError)throw state.healthError;
        return {status:'health',observation:{version:'1.4.0',runtime_sha256:f.digest,instance:'34'.repeat(16),policy:{accepting:true}}};
    },dispose(){disposed++;}});
    return {host,state,calls,count:(name:string)=>calls.filter(args=>args[1]===name).length,disposed:()=>disposed,cleanup:f.cleanup};
}
test('lone healthy connection runs zero diagnose and later full discovery obtains fresh inspect exactly once',async()=>{
    const f=await lazyDiscoveryFixture();
    try{
        const connection=await f.host.discoverForConnection();assert.equal(connection.diagnosis,undefined);
        assert.equal(f.count('inspect'),1);assert.equal(f.count('diagnose'),0);assert.equal(f.disposed(),1);
        const [first,second]=await Promise.all([f.host.discover(),f.host.discover()]);
        assert.deepEqual(first.diagnosis,{fixture:'full'});assert.deepEqual(second,first);
        assert.equal(f.count('inspect'),2,'finished connection-only attempt is not a persistent inspect cache');
        assert.equal(f.count('diagnose'),1);assert.equal(f.count('self-test'),1,'existing successful self-test cache retained');
    }finally{await f.cleanup();}
});
test('mandatory health and Stop failures retire connection-only attempts without diagnostics or poisoned reuse',async()=>{
    for(const failure of ['self-test','health','stop']){
        const f=await lazyDiscoveryFixture(),original=new Error('actual fixture health failure');
        try{
            f.state.failSelfTest=failure==='self-test';f.state.stopped=failure==='stop';f.state.healthError=failure==='health'?original:undefined;
            if(failure==='health')await assert.rejects(f.host.discoverForConnection(),error=>error===original);
            else await assert.rejects(f.host.discoverForConnection(),failure==='stop'?/service_explicitly_stopped/:/service_runtime_self_test_failed/);
            assert.equal(f.count('diagnose'),0);
            const before=f.count('inspect');f.state.failSelfTest=false;f.state.stopped=false;f.state.healthError=undefined;
            assert.equal((await f.host.discoverForConnection()).inspection?.payload_verified,true);
            assert.equal(f.count('inspect'),before+1);assert.equal(f.count('diagnose'),0);
        }finally{await f.cleanup();}
    }
});
test('connection waiters share mandatory work until the last health completes even after full settles',async()=>{
    const f=await lazyDiscoveryFixture(),entered=[lazySignal<void>(),lazySignal<void>(),lazySignal<void>()],release=[lazySignal<void>(),lazySignal<void>(),lazySignal<void>()];
    f.state.health=async number=>{if(number<=3){entered[number-1].resolve();await release[number-1].promise;}};
    let first:Promise<unknown>|undefined,second:Promise<unknown>|undefined,third:Promise<unknown>|undefined;
    try{
        first=f.host.discoverForConnection();second=f.host.discoverForConnection();
        await Promise.all([entered[0].promise,entered[1].promise]);release[0].resolve();await first;
        third=f.host.discoverForConnection();await entered[2].promise;
        assert.equal(f.count('inspect'),1,'third caller must share while second still owns health');assert.equal(f.count('diagnose'),0);
        release[2].resolve();await third;
        assert.deepEqual((await f.host.discover()).diagnosis,{fixture:'full'});assert.equal(f.count('inspect'),1);assert.equal(f.count('diagnose'),1);
        await f.host.discoverForConnection();assert.equal(f.count('inspect'),1,'settled full stays in active attempt until remaining connection finishes');
        release[1].resolve();await second;
        await f.host.discover();assert.equal(f.count('inspect'),2);assert.equal(f.count('diagnose'),2);
    }finally{release.forEach(gate=>gate.resolve());await Promise.allSettled([first,second,third]);await f.cleanup();}
});
''';p.write_text(s,encoding='utf-8',newline='\n')
