// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {EventEmitter} from 'node:events';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import test from 'node:test';
import {installInstalledControlTrace} from './installedControlTrace';

const frame=(value:unknown)=>{const body=Buffer.from(JSON.stringify(value)),result=Buffer.alloc(4+body.length);result.writeUInt32BE(body.length);body.copy(result,4);return result;};
function fixture(){
    const output=fs.mkdtempSync(path.join(os.tmpdir(),'airk-control-trace-')),install=path.join(output,'native-service'),control=path.join(install,'control'),runtimeSha256='1'.repeat(64),binary=path.join(install,'runtimes',runtimeSha256,'airk.exe');
    const args=['service','console','--runtime-dir',control,'--runtime-sha256',runtimeSha256];
    fs.mkdirSync(control,{recursive:true});fs.mkdirSync(path.dirname(binary),{recursive:true});fs.writeFileSync(binary,'fictional executable, never executed');
    const writes:any[][]=[],children:any[]=[],spawnCalls:any[][]=[];
    const spawn=function(this:unknown,...args:any[]){spawnCalls.push(args);const child:any=new EventEmitter();child.stdout=new EventEmitter();child.stdin={write:function(...input:any[]){writes.push(input);const callback=input.at(-1);if(typeof callback==='function')callback();return false;}};child.originalWrite=child.stdin.write;children.push(child);return child;};
    const host={spawn:spawn as any},dispose=()=>fs.rmSync(output,{recursive:true,force:true});
    const receipt=()=>JSON.parse(fs.readFileSync(path.join(output,`a-control-trace-${process.pid}.json`),'utf8'));
    return {output,install,control,binary,runtimeSha256,args,writes,children,spawnCalls,spawn,host,dispose,receipt};
}
test('trace passes exact native I/O through and retains only bounded operation metadata across fragments',()=>{
    const f=fixture();let time=1;const restore=installInstalledControlTrace({installDir:f.install,runtimeSha256:f.runtimeSha256,outputDirectory:f.output,role:'a'},{spawnHost:f.host,now:()=>time});
    try{
        const args=f.args,options={stdio:'pipe'},child:any=f.host.spawn(f.binary,args,options as any);assert.equal(child,f.children[0]);
        const id='a'.repeat(32),request=frame({id,method:{catalog_grant_draft:{credential:'NEVER_RETAIN_TOKEN',key:{nonce:'NEVER_RETAIN_NONCE'}}}});let callbacks=0;
        const callback=()=>{callbacks++;};assert.equal(child.stdin.write(request,callback),false);assert.equal(callbacks,1);assert.equal(f.writes[0][0],request);assert.equal(f.writes[0][1],callback);
        assert.equal(f.spawnCalls[0][1],args);assert.equal(f.spawnCalls[0][2],options);
        time=11;const response=frame({id,response:{status:'failure',code:'invalid_registry_request',body:'NEVER_RETAIN_BODY'}});
        child.stdout.emit('data',response.subarray(0,2));child.stdout.emit('data',response.subarray(2,9));child.stdout.emit('data',response.subarray(9));
        const receipt=f.receipt();assert.deepEqual(receipt.records,[{method:'catalog_grant_draft',status:'failure',code:'invalid_registry_request',elapsed_ms:10}]);
        const encoded=JSON.stringify(receipt);for(const privateValue of ['NEVER_RETAIN','credential','nonce',id,f.binary,f.control])assert.equal(encoded.includes(privateValue),false);
        child.emit('close',0,null);assert.equal(f.receipt().summary.closed_children,1);assert.equal(f.receipt().summary.pending,0);assert.equal(child.stdin.write,child.originalWrite);
    }finally{restore();assert.equal(f.host.spawn,f.spawn);f.dispose();}
});
test('foreign commands and malformed frames are ignored without changing native behavior or throwing',()=>{
    const f=fixture(),restore=installInstalledControlTrace({installDir:f.install,runtimeSha256:f.runtimeSha256,outputDirectory:f.output,role:'a'},{spawnHost:f.host});
    try{
        for(const args of [['runtime','inspect','--install-dir',f.install],['service','console','--runtime-dir',f.output,'--runtime-sha256',f.runtimeSha256],['service','console','--runtime-dir',f.control],[...f.args.slice(0,5),'2'.repeat(64)]]){
            const child:any=f.host.spawn(f.binary,args);assert.equal(child.stdin.write,child.originalWrite);child.stdin.write(frame({id:'b'.repeat(32),method:'request_epoch'}));child.emit('close');
        }
        assert.equal(fs.existsSync(path.join(f.output,`a-control-trace-${process.pid}.json`)),false);
        const child:any=f.host.spawn(f.binary,f.args);assert.doesNotThrow(()=>child.stdout.emit('data',Buffer.from([0,1,0,1])));
        child.stdin.write(frame({id:'c'.repeat(32),method:'request_epoch'}));child.emit('close');
        assert.equal(f.receipt().records[0].status,'transport_closed');
    }finally{restore();f.dispose();}
});
test('trace retains first failure and latest records when its finite record capacity is exceeded',()=>{
    const f=fixture(),restore=installInstalledControlTrace({installDir:f.install,runtimeSha256:f.runtimeSha256,outputDirectory:f.output,role:'a'},{spawnHost:f.host});
    try{
        const child:any=f.host.spawn(f.binary,f.args);
        for(let count=0;count<520;count++){
            const id=count.toString(16).padStart(32,'0');child.stdin.write(frame({id,method:count===1?'catalog_grant_draft':count===519?'catalog_host_context':'request_epoch'}));
            child.stdout.emit('data',frame({id,response:count===1?{status:'failure',code:'registry_busy'}:{status:'request_epoch',credential:'discarded'}}));
        }
        child.emit('close');const result=f.receipt();assert.equal(result.records.length,512);assert.equal(result.summary.seen,520);assert.equal(result.summary.dropped,8);
        assert.equal(result.records[0].method,'request_epoch');assert.equal(result.records[1].code,'registry_busy');assert.equal(result.records.at(-1).method,'catalog_host_context');
    }finally{restore();f.dispose();}
});
test('pending observation bound never blocks native writes and restore leaves later builtin ownership intact',()=>{
    const f=fixture(),restore=installInstalledControlTrace({installDir:f.install,runtimeSha256:f.runtimeSha256,outputDirectory:f.output,role:'a'},{spawnHost:f.host});
    try{
        const child:any=f.host.spawn(f.binary,f.args);
        for(let count=0;count<9;count++)child.stdin.write(frame({id:count.toString(16).padStart(32,'0'),method:'request_epoch'}));
        assert.equal(f.writes.length,9,'The trace must never limit the real transport');
        for(let count=0;count<9;count++)child.stdout.emit('data',frame({id:count.toString(16).padStart(32,'0'),response:{status:'request_epoch'}}));
        child.emit('close');assert.equal(f.receipt().records.length,8);assert.equal(f.receipt().summary.dropped_requests,1);
        const replacement=(()=>{throw new Error('later owner');}) as any;f.host.spawn=replacement;restore();assert.equal(f.host.spawn,replacement);
    }finally{restore();f.dispose();}
});
test('Windows extended native command and control paths retain the same exact owned trace', {skip:process.platform!=='win32'},()=>{
    const f=fixture(),restore=installInstalledControlTrace({installDir:f.install,runtimeSha256:f.runtimeSha256,outputDirectory:f.output,role:'a'},{spawnHost:f.host});
    try{
        const command=path.toNamespacedPath(f.binary),args=[...f.args];args[3]=path.toNamespacedPath(f.control);
        const child:any=f.host.spawn(command,args),id='d'.repeat(32),request=frame({id,method:'request_epoch'});
        assert.equal(f.spawnCalls[0][0],command);assert.equal(f.spawnCalls[0][1],args);
        child.stdin.write(request);assert.equal(f.writes[0][0],request);
        child.stdout.emit('data',frame({id,response:{status:'failure',code:'invalid_registry_request'}}));
        assert.equal(f.receipt().records[0].method,'request_epoch');assert.equal(f.receipt().records[0].code,'invalid_registry_request');
        child.emit('close');assert.equal(child.stdin.write,child.originalWrite);
    }finally{restore();f.dispose();}
});
