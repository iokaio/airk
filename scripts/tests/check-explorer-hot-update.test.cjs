// SPDX-License-Identifier: Apache-2.0
const test=require('node:test'),assert=require('node:assert/strict'),crypto=require('node:crypto'),fs=require('node:fs/promises'),os=require('node:os'),path=require('node:path');
const {execFileSync}=require('node:child_process');
const {qualify,verifyArchive,hasValue,readMailbox,publishMailbox,cleanOwnedExit}=require('../check-explorer-hot-update.cjs');
test('only an acknowledged clean owned Code exit qualifies cleanup; signal, nonzero and fallback never pass',()=>{
    assert.equal(cleanOwnedExit({code:0,signal:null},undefined,false),true);
    for(const [exit,error,forced]of [[undefined,undefined,false],[{code:1,signal:null},undefined,false],[{code:null,signal:'SIGTERM'},undefined,false],[{code:0,signal:null},'quit not acknowledged',false],[{code:0,signal:null},undefined,true],[{code:0,signal:'SIGTERM'},undefined,false]])assert.equal(cleanOwnedExit(exit,error,forced),false);
});
test('pending readiness and replies cannot complete polling and atomic mailbox publication exposes complete JSON',async()=>{
    const directory=await fs.mkdtemp(path.join(os.tmpdir(),'airk-hot-mailbox-'));
    try{
        for(const name of ['driver-ready','reply-1']){
            const filename=path.join(directory,name+'.json');
            assert.equal(hasValue(await readMailbox(filename)),false,'missing readiness/reply must remain pending');
            await publishMailbox(filename,null);assert.equal(hasValue(await readMailbox(filename)),false,'null cannot satisfy readiness/reply');
            await fs.rm(filename);
            const expected={value:{text:'complete'.repeat(100000)}};
            const publishing=publishMailbox(filename,expected);
            for(let i=0;i<8;i++){const observed=await readMailbox(filename);if(hasValue(observed))assert.deepEqual(observed,expected);}
            await publishing;assert.deepEqual(await readMailbox(filename),expected);
        }
        assert.equal(hasValue(false),false);assert.equal(hasValue({value:null}),true,'an actual acknowledged void reply is complete');
        assert.equal((await fs.readdir(directory)).some(name=>name.includes('.stage-')),false);
    }finally{await fs.rm(directory,{recursive:true,force:true});}
});
test('actual hot update qualification rejects restart, duplicate panels, wrong assets, lost route or lost dismissal',()=>{
    const baseline={identity:'a'.repeat(64)},candidate={identity:'b'.repeat(64)},host={pid:100,boot:'actual-host-boot',profile:'owned-profile'};
    const before={host,tabs:1,asset:'resource/'+baseline.identity+'/explorerWebview.js',route:'#indexing/window'};
    const after={host:{...host},tabs:1,asset:'resource/'+candidate.identity+'/explorerWebview.js',route:before.route,noticeAbsent:true,reopenedNoticeAbsent:true};
    assert.equal(qualify(before,after,baseline,candidate).qualified,true);
    for(const change of [v=>v.host.pid++,v=>v.host.boot='new-host',v=>v.host.profile='foreign',v=>v.tabs=2,v=>v.asset=before.asset,v=>v.route='#overview',v=>v.noticeAbsent=false,v=>v.reopenedNoticeAbsent=false]){
        const crossed=structuredClone(after);change(crossed);assert.throws(()=>qualify(before,crossed,baseline,candidate));
    }
    assert.throws(()=>qualify(before,after,baseline,baseline));
});
test('real archive verifier checks reviewed whole-package and complete captured inventory bytes',async()=>{
    const directory=await fs.mkdtemp(path.join(os.tmpdir(),'airk-hot-archive-')),archive=path.join(directory,'fixture.vsix');
    const create=String.raw`import hashlib,json,sys,zipfile
p,mutate=sys.argv[1:]
m=json.dumps({'name':'airk','publisher':'iokaio','version':'1.8.14','main':'./out/extension.js'})
files={'package.json':m,'out/extension.js':'require("./extensionUpdateLoader")','out/extensionUpdateLoader.js':'class ExtensionUpdateLoader {}','out/explorerWebview.js':'actual-fixture-ui'}
v={'schema':'airk-extension-update/1','version':'1.8.14','package_manifest':m,'files':{n:hashlib.sha256(b.encode()).hexdigest() for n,b in files.items()}}
if mutate=='changed':files['out/explorerWebview.js']='changed-after-seal'
if mutate=='no-loader':files['out/extensionUpdateLoader.js']='no bootstrap'
with zipfile.ZipFile(p,'w') as z:
 for n,b in files.items():z.writestr('extension/'+n,b)
 z.writestr('extension/airk-update.json',json.dumps(v))
`;
    try{
        const run=mode=>{execFileSync(process.env.PYTHON??'python',['-c',create,archive,mode]);return crypto.createHash('sha256').update(require('node:fs').readFileSync(archive)).digest('hex');};
        const digest=run('valid'),result=JSON.parse(execFileSync(process.env.PYTHON??'python',['-c',verifyArchive,archive,digest],{encoding:'utf8'}));
        assert.equal(result.archive_sha256,digest);assert.match(result.identity,/^[a-f0-9]{64}$/);
        assert.throws(()=>execFileSync(process.env.PYTHON??'python',['-c',verifyArchive,archive,'0'.repeat(64)],{stdio:'pipe'}));
        for(const mode of ['changed','no-loader']){const hash=run(mode);assert.throws(()=>execFileSync(process.env.PYTHON??'python',['-c',verifyArchive,archive,hash],{stdio:'pipe'}));}
    }finally{await fs.rm(directory,{recursive:true,force:true});}
});
