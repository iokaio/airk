# SPDX-License-Identifier: Apache-2.0
import difflib,hashlib,json
from pathlib import Path
BASE=Path(__file__).resolve().parent
WORK=BASE/'p10-worktree'
OUT=BASE/'p14-legacy-editor-driver-proposed-r1'
OUT.mkdir(exist_ok=False)
paths=['packages/vscode/src/test/installedEditor.ts','packages/vscode/src/test/installedEditorContracts.ts','packages/vscode/src/test/installedEditorContracts.test.ts','scripts/check-native-editor.cjs']
sources={p:(WORK/p).read_text() for p in paths}
def replace(path,old,new):
 assert sources[path].count(old)==1,(path,old[:80])
 sources[path]=sources[path].replace(old,new)
p=paths[1]
replace(p,"export function installedSelector",'''export class InstalledNativeReplyError extends Error {
    readonly code?:string;
    constructor(readonly reply:unknown,body?:any){super('Native MCP refused: '+JSON.stringify(reply));const code=body?.error?.code??body?.code??(reply as any)?.error?.data?.code;this.code=typeof code==='string'?code:undefined;}
}
/** Start is an explicit user action; close the manual transport before Stop. */
export async function installedLegacyLifecycle(start:()=>Promise<unknown>,read:()=>Promise<void>,stop:()=>Promise<void>,evidence:any,mark:(name:string,value:any)=>Promise<unknown>):Promise<void>{
    await start();
    let primary:unknown;
    try{await read();}catch(error){primary=error;throw error;}finally{
        try{await stop();}catch(error){
            evidence.legacy_stop_failure={error:String(error),primary_preserved:primary!==undefined};
            await mark('retained-evidence',evidence);
            if(primary===undefined)throw error;
        }
    }
}
export function installedSelector''')
replace(p,"    const body=JSON.parse(result.content[0].text);",'''    let body:any;
    try{body=JSON.parse(result.content[0].text);}catch(error){
        // Classic remote bridges can return a genuine plaintext error. Preserve
        // that complete refusal without inventing a typed compatibility code.
        if(result.isError===true)throw new InstalledNativeReplyError(wire);
        throw error; // Successful tool bodies must still satisfy the JSON contract.
    }''')
p=paths[0]
replace(p,"installedRevocation, finishInstalledClient }", "installedRevocation, finishInstalledClient, InstalledNativeReplyError as NativeReplyError, installedLegacyLifecycle }")
start=sources[p].index('class NativeReplyError extends Error {');end=sources[p].index('\n\nasync function waitFor',start)
sources[p]=sources[p][:start]+sources[p][end+2:]
replace(p,"        const client=new Mcp(definition);\n        try{",'''        await noSelection('Existing manual agent configuration owns its connection before explicit service start');
        await vscode.commands.executeCommand('airk.collections');
        await installedLegacyLifecycle(
        async()=>{await ui('startService',{installDir:fixture.install_dir,runtimeSha256:fixture.candidate.install_manifest_sha256},'#manage/service');checks.push('explicit trusted Start service on the verified legacy installation');},
        async()=>{
        const client=new Mcp(definition);
        try{''')
replace(p,"        assert.equal(createHash('sha256').update(await fs.readFile(fixture.legacy_config.path)).digest('hex'),fixture.legacy_config.sha256);",'''        },async()=>{
            const stop=await ui('serviceStop',{installDir:fixture.install_dir,runtimeSha256:fixture.candidate.install_manifest_sha256},'#manage/service');
            await fs.writeFile(path.join(folder,'native-stop.json'),JSON.stringify(stop,null,2));
            evidence.native.push({ownedLegacyServiceStop:stop});
        },evidence,mark);
        assert.equal(createHash('sha256').update(await fs.readFile(fixture.legacy_config.path)).digest('hex'),fixture.legacy_config.sha256);''')
p=paths[2]
sources[p]+='''
test('plaintext native refusal retains complete raw envelope without a compatibility code',()=>{
    const reply={result:{isError:true,content:[{type:'text',text:'AIRK direct project connection failed.'}]}};
    let refusal:unknown;try{installedToolBody(reply);}catch(error){refusal=error;}
    assert.ok(refusal instanceof InstalledNativeReplyError);assert.equal(refusal.reply,reply);assert.equal(refusal.code,undefined);
    assert.throws(()=>installedToolBody({result:{...reply.result,isError:false}}),SyntaxError);
    assert.throws(()=>installedToolBody({result:{...reply.result,isError:'true'}}));
});
test('legacy lifecycle explicitly starts once and closes the manual read before one Stop',async()=>{
    const order:string[]=[],evidence:any={};
    await installedLegacyLifecycle(async()=>{order.push('start');},async()=>{order.push('read');order.push('transport closed');},async()=>{order.push('stop');},evidence,async()=>{});
    assert.deepEqual(order,['start','read','transport closed','stop']);assert.deepEqual(evidence,{});
});
test('failed legacy read remains primary when its single Stop also fails',async()=>{
    const primary=new Error('manual read failed'),stopFailure=new Error('Stop unconfirmed'),order:string[]=[],evidence:any={},saved:any[]=[];
    await assert.rejects(installedLegacyLifecycle(async()=>{order.push('start');},async()=>{order.push('read closed');throw primary;},async()=>{order.push('stop');throw stopFailure;},evidence,async(_name,value)=>{saved.push(structuredClone(value));}),error=>error===primary);
    assert.deepEqual(order,['start','read closed','stop']);assert.equal(saved[0].legacy_stop_failure.primary_preserved,true);
    assert.match(saved[0].legacy_stop_failure.error,/Stop unconfirmed/);
    await assert.rejects(installedLegacyLifecycle(async()=>{},async()=>{},async()=>{throw stopFailure;},{},async()=>{}),error=>error===stopFailure);
});
'''
replace(p,'finishInstalledClient}', 'finishInstalledClient,InstalledNativeReplyError,installedLegacyLifecycle}')
p=paths[3]
replace(p,"const execute=promisify(execFile),services=[];", "const execute=promisify(execFile),services=[],legacyServices=new Map();")
replace(p,"        const install=path.join(folder,'native-service');if(!await exists(install))continue;",'''        const legacy=legacyServices.get(folder),install=legacy?.install_dir??path.join(folder,'native-service');if(!await exists(install))continue;''')
replace(p,"            const observation=JSON.parse(result.stdout),owned=await fs.realpath(folder),control=await canonicalOwned(owned,observation.control_dir);",'''            const observation=JSON.parse(result.stdout),owned=await fs.realpath(legacy?.fixture_root??folder),control=await canonicalOwned(owned,observation.control_dir);
            if(legacy){
                if(legacy.qualification?.status!=='qualified_native_fixture'||install!==legacy.install_dir||control!==await canonicalOwned(owned,legacy.control_dir)||captured.namespace!==legacy.native.inspect.registry_namespace)throw new Error('Crossed qualified legacy Stop installation');
                if(sha(await fs.readFile(legacy.legacy_config.path))!==legacy.legacy_config.sha256)throw new Error('Legacy configuration changed during lifecycle');
                for(const [name,digest]of Object.entries(legacy.source_hashes_before)){const file=await ownedPath(legacy.source_root,path.resolve(legacy.source_root,name));if(sha(await fs.readFile(file))!==digest)throw new Error('Legacy source changed during lifecycle');}
            }''')
replace(p,"        if(!legacy)services.push(folder);const storage", "        services.push(folder);if(legacy)legacyServices.set(folder,legacy);const storage")
replace(p,"                        if(request.action==='serviceStop'){\n                            await canonicalOwned(fixture,request.installDir);\n                            if(request.installDir!==path.join(folder,'native-service')||request.runtimeSha256!==bundle.digest)throw new Error('Crossed owned Stop fixture binding');",'''                        if(request.action==='serviceStop'||(legacy&&request.action==='startService')){
                            if(legacy){
                                if(legacy.qualification?.status!=='qualified_native_fixture'||request.installDir!==legacy.install_dir||request.runtimeSha256!==legacy.candidate.install_manifest_sha256||request.runtimeSha256!==bundle.digest)throw new Error('Crossed qualified legacy lifecycle binding');
                                await canonicalOwned(await fs.realpath(legacy.fixture_root),request.installDir);
                            }else{
                                await canonicalOwned(fixture,request.installDir);
                                if(request.installDir!==path.join(folder,'native-service')||request.runtimeSha256!==bundle.digest)throw new Error('Crossed owned Stop fixture binding');
                            }''')
replace(p,"                            await fs.writeFile(path.join(folder,'native-pre-stop-inspect.json'),JSON.stringify(metadata,null,2));",'''                            if(legacy&&(metadata.registry_namespace!==legacy.native.inspect.registry_namespace||await canonicalOwned(await fs.realpath(legacy.fixture_root),metadata.control_dir)!==await canonicalOwned(await fs.realpath(legacy.fixture_root),legacy.control_dir)))throw new Error('Legacy lifecycle changed native namespace/control');
                            await fs.writeFile(path.join(folder,request.action==='serviceStop'?'native-pre-stop-inspect.json':'native-pre-start-inspect.json'),JSON.stringify(metadata,null,2));''')
patch='';rows=[]
for p in paths:
 old=(WORK/p).read_bytes();new=sources[p].encode();dest=OUT/'source'/p;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(new)
 before=OUT/'base'/p;before.parent.mkdir(parents=True,exist_ok=True);before.write_bytes(old)
 rows.append({'path':p,'base_sha256':hashlib.sha256(old).hexdigest(),'proposed_sha256':hashlib.sha256(new).hexdigest()})
 patch+=''.join(difflib.unified_diff(old.decode().splitlines(True),sources[p].splitlines(True),fromfile='a/'+p,tofile='b/'+p))
(OUT/'legacy-editor.patch').write_text(patch,encoding='utf-8',newline='\n')
m={'status':'inert proposal; no tracked edits/compilation/native execution','files':rows,'patch_sha256':hashlib.sha256((OUT/'legacy-editor.patch').read_bytes()).hexdigest(),'cause':'Fixture owner deliberately completed Stop; legacy editor branch tried retained manual read without explicit Start. Generic remote refusal was masked by JSON parser. New Start requires matching qualified private installation; manual closes before one exact Stop; legacy now enters native cleanup validation.','scope':'Test-only lifecycle/parsing/runner correction; no production/native/API changes. Supported launcher still must retrieve full sealed imported scope. Pinned predecessor requires actual typed incompatible refusal; plaintext code remains undefined and cannot pass that assertion.'}
(OUT/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(json.dumps({'directory':str(OUT),'patch_sha256':m['patch_sha256'],'manifest_sha256':hashlib.sha256((OUT/'manifest.json').read_bytes()).hexdigest()}))
