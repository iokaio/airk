# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib,subprocess
A=Path(__file__).parent;W=A/'p10-worktree';O=A/'p14-editor-ui-contract-proposed-r1'
(O/'base').mkdir(parents=True,exist_ok=True);(O/'source').mkdir(exist_ok=True)
files={}
def put(name,s):
 b=(W/name).read_bytes() if (W/name).exists() else b''
 if b'\r\n' in b:s=s.replace('\r\n','\n').replace('\n','\r\n')
 for folder,data in [('base',b),('source',s.encode())]:
  p=O/folder/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
 files[name]=(b,s.encode())
name='scripts/editor-ui.cjs';s=(W/name).read_text(encoding='utf-8')
anchor='const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));'
helper=r'''// The driver is a separate private editor process, not a fork of the host's VS Code role.
function privateEditorEnvironment(parent=process.env){
    const value={...parent,RUST_BACKTRACE:'1'};
    for(const key of Object.keys(value)){const name=key.toUpperCase();if(name.startsWith('VSCODE_')||name==='ELECTRON_RUN_AS_NODE'||name==='NODE_COMPILE_CACHE')delete value[key];}
    return value;
}
function formFillExpressions(button,values){
    if(typeof button!=='string'||!button.length||button.length>256||!values||typeof values!=='object'||Array.isArray(values))throw new Error('Invalid exact private form inputs');
    const entries=Object.entries(values);
    if(!entries.length||entries.length>32||entries.some(([label,value])=>!label.length||label.length>256||!(typeof value==='boolean'||typeof value==='string'&&value.length<=4096)))throw new Error('Private form input bound/type exceeded');
    const lookup=`const buttons=[...document.querySelectorAll('button')].filter(b=>b.textContent.trim()===${JSON.stringify(button)});const form=buttons.length===1?buttons[0].closest('form'):null;if(!form)return false;const fields=Object.entries(${JSON.stringify(values)}).map(([label,value])=>{const labels=[...form.querySelectorAll('label')].filter(n=>[...n.childNodes].filter(c=>c.nodeType===Node.TEXT_NODE).map(c=>c.textContent).join('').trim()===label);return {value,node:labels.length===1?labels[0].querySelector('input,select'):null};});if(fields.some(({node,value})=>!node||(typeof value==='boolean')!==(node.type==='checkbox')))return false;`;
    return {ready:`(()=>{${lookup}return true;})()`,
        apply:`(()=>{${lookup}for(const {node,value}of fields){const proto=node.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(proto,node.type==='checkbox'?'checked':'value').set.call(node,value);node.dispatchEvent(new Event(node.tagName==='SELECT'||node.type==='checkbox'?'change':'input',{bubbles:true}));}return true;})()`,
        readback:`(()=>{${lookup}return fields.every(({node,value})=>(node.type==='checkbox'?node.checked:node.value)===value);})()`};
}
'''
assert s.count(anchor)==1;s=s.replace(anchor,helper+anchor)
start=s.index('    async function fill(button,values){');end=s.index('    async function catalog(){',start)
s=s[:start]+'''    async function fill(button,values){
        const expressions=formFillExpressions(button,values);
        // All exact fields must exist before any setter; a captured-context
        // outcome is still dispatched once and is never automatically replayed.
        await dispatch(explorerPredicate,expressions.ready,expressions.apply);
        await until(async()=>{const found=await service(expressions.readback);return found?.value===true?true:undefined;},'exact private form values before submit');
    }
'''+s[end:]
old="{'Index ID from this grant':request.index,'Project IDs from this grant (1–32, comma separated)':request.project}"
new="{'Optional index subset from this grant (1–32 IDs; blank includes all permitted members)':request.index,'Project IDs from this grant (1–32, comma separated)':request.project}"
assert s.count(old)==1;s=s.replace(old,new)
assert 'module.exports={editorUi,until};' in s;s=s.replace('module.exports={editorUi,until};','module.exports={editorUi,until,formFillExpressions,privateEditorEnvironment};');put(name,s)
name='scripts/check-native-editor.cjs';s=(W/name).read_text(encoding='utf-8')
assert "const {editorUi}=require('./editor-ui.cjs');" in s;s=s.replace("const {editorUi}=require('./editor-ui.cjs');","const {editorUi,privateEditorEnvironment}=require('./editor-ui.cjs');")
old="const environment={...process.env,RUST_BACKTRACE:'1'};delete environment.ELECTRON_RUN_AS_NODE;delete environment.VSCODE_IPC_HOOK_CLI;"
assert old in s;s=s.replace(old,"const environment=privateEditorEnvironment(process.env); // Child only: never inherit the normal editor's bootstrap/cache/IPC role.")
put(name,s)
put('scripts/tests/editor-ui.test.cjs',r'''// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs'),path=require('node:path');
const {formFillExpressions,privateEditorEnvironment}=require('../editor-ui.cjs');
const label='Optional index subset from this grant (1–32 IDs; blank includes all permitted members)';
function fixture(specs){
    const writes=[],events=[];
    class Input{
        constructor(type='text'){this.type=type;this.tagName='INPUT';this.current='';this.flag=false;}
        get value(){return this.current;}set value(value){writes.push(value);this.current=value;}
        get checked(){return this.flag;}set checked(value){writes.push(value);this.flag=value;}
        dispatchEvent(event){events.push(event.type);}
    }
    class Select extends Input{constructor(){super();this.tagName='SELECT';}}
    const inputs=specs.map(row=>new Input(row.type));
    const labels=specs.map((row,i)=>({childNodes:[{nodeType:3,textContent:row.label}],querySelector:()=>inputs[i]}));
    const form={querySelectorAll:()=>labels},button={textContent:'Use Grant for this window',closest:()=>form};
    const context=vm.createContext({document:{querySelectorAll:()=>[button]},Node:{TEXT_NODE:3},HTMLInputElement:Input,HTMLSelectElement:Select,Event:class{constructor(type){this.type=type;}}});
    return {run:expression=>vm.runInContext(expression,context),inputs,writes,events};
}
test('actual optional-index label remains aligned with renderer; exact requested subset is preserved',()=>{
    const source=fs.readFileSync(path.resolve(__dirname,'../../packages/vscode/src/explorerWebview/indexing.tsx'),'utf8');assert.ok(source.includes(label));
    const values={[label]:'idx_'+'1'.repeat(32),'Project IDs from this grant (1–32, comma separated)':'prj_'+'2'.repeat(32)};
    const f=fixture(Object.keys(values).map(label=>({label}))),e=formFillExpressions('Use Grant for this window',values);
    assert.equal(f.run(e.ready),true);assert.equal(f.run(e.apply),true);assert.equal(f.run(e.readback),true);
    assert.deepEqual(f.writes,Object.values(values));assert.deepEqual(f.events,['input','input']);
});
test('missing later field causes no earlier write, not partial fill or a submit',()=>{
    const f=fixture([{label}]),e=formFillExpressions('Use Grant for this window',{[label]:'exact-index','Missing later field':'exact-project'});
    assert.equal(f.run(e.ready),false);assert.equal(f.run(e.apply),false);assert.deepEqual(f.writes,[]);assert.deepEqual(f.events,[]);
});
test('old label and duplicate labels refuse without writes',()=>{
    const f=fixture([{label}]),old=formFillExpressions('Use Grant for this window',{'Index ID from this grant':'exact-index'});
    assert.equal(f.run(old.ready),false);assert.equal(f.run(old.apply),false);assert.deepEqual(f.writes,[]);
    const dup=fixture([{label},{label}]),e=formFillExpressions('Use Grant for this window',{[label]:'exact-index'});
    assert.equal(dup.run(e.apply),false);assert.deepEqual(dup.writes,[]);
});
test('readonly readback catches lost exact values before a separate submit',()=>{
    const f=fixture([{label}]),e=formFillExpressions('Use Grant for this window',{[label]:'exact-index'});
    assert.equal(f.run(e.apply),true);f.inputs[0].current='';assert.equal(f.run(e.readback),false);assert.deepEqual(f.events,['input']);
});
test('checkbox booleans retain type and change event; invalid input types refuse',()=>{
    const f=fixture([{label:'Watch',type:'checkbox'}]),e=formFillExpressions('Use Grant for this window',{Watch:false});
    assert.equal(f.run(e.apply),true);assert.equal(f.run(e.readback),true);assert.deepEqual(f.events,['change']);
    assert.throws(()=>formFillExpressions('Use Grant for this window',{Watch:[]}));
});
test('child environment isolates inherited editor role/cache/IPC without changing host or explicit fixture scope',()=>{
    const parent={PATH:'owned-tools',AIRK_EDITOR_SCENARIOS:'same',AIRK_PRIVATE_CENSUS_BASE:'fictional-private',VSCODE_CWD:'normal-code',VSCODE_NODE_COMPILE_CACHE_ROOT:'normal-cache',VSCODE_CODE_CACHE_PATH:'normal-user-cache',VSCODE_ESM_ENTRYPOINT:'extensionHostProcess',VSCODE_IPC_HOOK_CLI:'normal-hook',vscode_fake_cache:'case-variant',ELECTRON_RUN_AS_NODE:'1',NODE_COMPILE_CACHE:'normal-node-cache'};
    const before={...parent},child=privateEditorEnvironment(parent);
    assert.deepEqual(parent,before);assert.ok(!Object.keys(child).some(key=>key.toUpperCase().startsWith('VSCODE_')));
    assert.equal(child.ELECTRON_RUN_AS_NODE,undefined);assert.equal(child.NODE_COMPILE_CACHE,undefined);
    assert.equal(child.PATH,parent.PATH);assert.equal(child.AIRK_EDITOR_SCENARIOS,'same');assert.equal(child.AIRK_PRIVATE_CENSUS_BASE,'fictional-private');
    assert.equal({...child,ELECTRON_RUN_AS_NODE:'1'}.ELECTRON_RUN_AS_NODE,'1','explicit CLI role remains possible per canonical caller');
});
''')
patch='';manifest={'status':'INERT_TEST_TOOL_PROPOSAL_NO_EXECUTION','files':[],'diagnosis_sha256':hashlib.sha256((A/'p14-editor-dispatch-independent-diagnosis-r1.json').read_bytes()).hexdigest(),
 'validation_pending':['node --check scripts/editor-ui.cjs','node --check scripts/check-native-editor.cjs','node --test scripts/tests/editor-ui.test.cjs','Fresh canonical actual five and legacy scenarios after old fixture cleanup; never replay old unknown action'],
 'limits':['Six pure VM/environment tests proposed, not run; no actual browser result inferred','Product native and extension bytes unchanged, reuse49170VSIX/three passing package stages','Original Unknown dispatch and failure remain unchanged; fresh profiles/requests required']}
for name,(b,s) in files.items():
 manifest['files'].append({'path':name,'base_sha256':hashlib.sha256(b).hexdigest() if b else None,'proposed_sha256':hashlib.sha256(s).hexdigest()})
 patch+=''.join(difflib.unified_diff(b.decode().splitlines(keepends=True),s.decode().splitlines(keepends=True),fromfile='a/'+name if b else '/dev/null',tofile='b/'+name))
(O/'editor-ui.patch').write_bytes(patch.encode());manifest['patch_sha256']=hashlib.sha256(patch.encode()).hexdigest()
r=subprocess.run(['git','-c','safe.directory='+str(W.resolve()),'-C',str(W),'apply','--check',str((O/'editor-ui.patch').resolve())],capture_output=True,text=True);assert r.returncode==0,r.stderr
manifest['static_validation']='Three current bases/proposed hashes +applycheck0; no Node/editor/native execution'
(O/'manifest.json').write_bytes((json.dumps(manifest,indent=2)+'\n').encode())
print('patch',manifest['patch_sha256']);print('manifest',hashlib.sha256((O/'manifest.json').read_bytes()).hexdigest())
