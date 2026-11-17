# SPDX-License-Identifier: Apache-2.0
import hashlib,json,re
from pathlib import Path
root=Path.cwd()
out=root/'artifacts/mcp-optimizations-1.8.3/review'
out.mkdir(exist_ok=True)
source=(root/'crates/airk-kernel/src/workbench.rs').read_text(encoding='utf-8')
fields_text=source[source.index('fn fields(tool:'):source.index('/// Forms and agents inspect')]
fields={}
for names,body in re.findall(r'((?:"[^"]+"\s*\|\s*)*"[^"]+")\s*=>\s*&\[(.*?)\]',fields_text,re.S):
    for name in re.findall(r'"([^"]+)"',names):fields[name]=re.findall(r'"([^"]+)"',body)
required={'search':['query'],'references':['name'],'read_file':['path','start_line','end_line'],'file_info':['path'],'outline':['path'],'context':['path'],'changes':['since'],'relationships':['symbol']}
def schema(key):
    if key in ['limit','max_bytes','start_line','end_line','line','before','after','hops']:
        maximum={'limit':100,'hops':3,'before':40,'after':40,'line':4294967295,'start_line':4294967295,'end_line':4294967295}.get(key,65536)
        return {'type':'integer','minimum':0 if key in ['before','after'] else 1,'maximum':maximum}
    if key in ['include_count','include_files','compact']:return {'type':'boolean'}
    enums={'mode':['lexical','vector','hybrid'],'direction':['incoming','outgoing'],'name_match':['exact','suffix','contains'],
        'evidence_kind':['code','document','configuration','unknown'],'order':['index','path','code_first']}
    if key in enums:return {'type':'string','enum':enums[key]}
    if key in ['cursor','language','exclude_path_prefix']:return {'type':'string','minLength':1,'maxLength':{'cursor':32768,'language':128,'exclude_path_prefix':4096}[key]}
    return {'type':'string','maxLength':4096}
operations={name:{'type':'object','additionalProperties':False,'properties':{key:schema(key) for key in sorted(keys)},'required':required.get(name,[])} for name,keys in sorted(fields.items())}
assert fields['search']==['query','mode','path_prefix','language','evidence_kind','exclude_path_prefix','limit','max_bytes']
assert 'compact' in fields['symbols'] and 'order' in fields['references']
(out/'operation-catalog-source-verification.json').write_text(json.dumps({'basis':'Static field shapes verified against current native operation_catalog/argument_schema; not a runtime observation',
    'source':'crates/airk-kernel/src/workbench.rs','source_sha256':hashlib.sha256((root/'crates/airk-kernel/src/workbench.rs').read_bytes()).hexdigest(),'operations':operations},indent=2)+'\n')
renderer=(root/'scripts/render-workbench.cjs').read_text(encoding='utf-8')
renderer=re.sub(r"const root=path.resolve\(__dirname,'\.\.'\),output=.*?;",lambda _m:f"const root={json.dumps(str(root))},output={json.dumps(str(out))};",renderer,count=1)
branch=r'''
        if(true){
            const operations=JSON.parse(await fs.readFile(path.join(output,'operation-catalog-source-verification.json'),'utf8')).operations;
            const pub=publication;
            const resultSteps=[
                {id:'refs',tool:'references',status:'complete',arguments:{name:'Session',path_prefix:'src/'},result:{publication:{token:pub},evidence:{truncated:true,resolution:'textual'},page:{cursor:'fictional-page',more:true,count_unit:'matching_lines',files:[{path:'src/session.rs',count:2}],files_truncated:false,match_count:{total_matches:null,lower_bound:2,complete:false,scope:'verified_lexical_candidates'},coverage:{basis:'verified_lexical_candidates',candidate_scan_complete:false,query_complete:false,query_total_matches:null,token_boundary_complete:false,line_completeness:'complete_source_lines'}}}},
                {id:'definition',tool:'search',status:'complete',result:{publication:{token:pub},definitions:[{name:'Session',total_matches:null,lower_bound:1,complete:false,evidence_scope:'native_definition_records',matches:[{symbol_id:'syntax:session',citation:{path:'src/session.rs',content_hash:hash,start_line:5,end_line:8}}]}]}},
                {id:'methods',tool:'relationships',status:'complete',result:{publication:{token:pub},suggested_targets:[{target:'Session::spawn',citation:{path:'src/session.rs'},coverage:'Written method target; call matches remain syntax candidates.'}]}},
            ];
            const evidence=[{step:'refs',publication:pub,text:'let session = Session::spawn(connection);',citation:{path:'src/session.rs',start_line:14,end_line:14,content_hash:hash,location:{Lines:{start:14,end:14}}}},
                {step:'refs',publication:pub,text:'let reopened: Session = restore_session(connection);',citation:{path:'src/session.rs',start_line:32,end_line:32,content_hash:hash,location:{Lines:{start:32,end:32}}}}];
            const formValues={
                search:{query:'Session connection lifecycle',mode:'hybrid',path_prefix:'src/',language:'rust',evidence_kind:'code',exclude_path_prefix:'src/generated/',limit:'20',max_bytes:'16384'},
                symbols:{name:'Session',path_prefix:'src/',language:'rust',name_match:'suffix',include_count:'true',compact:'true',limit:'20',max_bytes:'16384'},
                references:{name:'Session',path_prefix:'src/',language:'rust',evidence_kind:'code',exclude_path_prefix:'src/generated/',order:'code_first',include_files:'true',include_count:'true',limit:'40',max_bytes:'24576'},
            };
            const labels={path_prefix:'Path prefix',exclude_path_prefix:'Exclude path prefix',evidence_kind:'Evidence type',name_match:'Name matching',compact:'Compact results',include_count:'Show count precision',include_files:'Summarize files',max_bytes:'Response byte budget'};
            const file=path.join(scratch,'review.html');await fs.writeFile(file,html('results',true));
            const load=async(width,height)=>{
                await call('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
                await call('Page.navigate',{url:pathToFileURL(file).href});await until(`!!document.querySelector('.composer textarea')`);
                await evaluate(`window.fixture.operations=${JSON.stringify(operations)};window.fixture.projects[0].name='Session example';window.fixture.session.title='Explore Session';window.fixture.session.turns[0].input='How is Session used?';window.fixture.session.turns[0].envelope={...window.fixture.session.turns[0].envelope,executed:true,status:'partial',steps:${JSON.stringify(resultSteps)},evidence:${JSON.stringify(evidence)},warnings:[],completeness:['More reference candidates remain available.']};window.fixture.cards['turn-1']=${JSON.stringify(evidence.map((row,i)=>({id:'record-'+i,path:row.citation.path,hash,start:row.citation.start_line,end:row.citation.end_line,text:row.text,location:row.citation.location,step:row.step,publication:pub})))};window.fixture.continuations={'turn-1':[{id:'page-0',tool:'references',callsRemaining:9}]};window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}));`);
                await until(`document.querySelectorAll('.card').length===2`);
            };
            const capture=async(name,clip)=>{const png=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false,...(clip?{clip}: {})});await fs.writeFile(path.join(output,name),Buffer.from(png.data,'base64'));};
            for(const [index,operation] of ['search','symbols','references'].entries()){
                await load(640,3200);
                await evaluate(`document.querySelector('[aria-label="Add scope, recipe or tool"]').click()`);await until(`!!document.querySelector('.tool-form form')`);
                await evaluate(`(()=>{const select=document.querySelector('.tool-form [aria-label="Operation"]');select.value=${JSON.stringify(operation)};select.dispatchEvent(new Event('change',{bubbles:true}));})()`);
                await delay(50);
                for(const [key,value]of Object.entries(formValues[operation])){
                    const label=labels[key]??key.replaceAll('_',' ');
                    await evaluate(`(()=>{const field=[...document.querySelectorAll('.tool-form input,.tool-form select')].find(field=>field.getAttribute('aria-label')===${JSON.stringify(label)});if(!field)throw new Error('Missing native field '+${JSON.stringify(label)});field.value=${JSON.stringify(value)};field.dispatchEvent(new Event(field.tagName==='SELECT'?'change':'input',{bubbles:true}));})()`);await delay(15);
                }
                const geometry=await evaluate(`(()=>{const menu=document.querySelector('#evidence-options'),form=document.querySelector('.tool-form'),r=menu.getBoundingClientRect();return {top:r.top,bottom:r.bottom,height:r.height,formClient:form.clientHeight,formScroll:form.scrollHeight,windowHeight:innerHeight}})()`);
                assert.ok(geometry.formScroll<=geometry.formClient+2,'Full form is visible at a tall standard browser viewport');
                const y=Math.max(0,Math.floor(geometry.top)-14),height=Math.min(3200-y,Math.ceil(geometry.height)+170);
                assert.equal(await evaluate(`!!document.querySelector('[aria-label="Reset this tool form"] svg')`),true,'Actual native-family Reset icon is present');
                await capture(`0${index+7}-${operation}-form-reset-640.png`,{x:0,y,width:640,height,scale:1});
                checks.push({operation,width:640,viewportHeight:3200,capture:'Native menu/form/composer browser clip',nativeFields:Object.keys(operations[operation].properties),allFormFieldsVisible:true});
            }
            await load(480,1000);
            await evaluate(`document.querySelector('.transcript').scrollTop=0`);await capture('10-session-results-sidebar-480.png');
            await load(640,2200);
            await evaluate(`document.querySelector('.transcript').scrollTop=0`);await capture('11-session-results-full-640.png');
            assert.equal(await evaluate(`document.body.textContent.includes('whole-query total')&&document.body.textContent.includes('Find callers of Session::spawn')`),true);
            await fs.writeFile(path.join(output,'review-summary.json'),JSON.stringify({basis:'Actual compiled product Workbench UI, fictional original data and source-verified native operation_catalog field shapes. No installer, native benchmark or installed-editor qualification.',checks},null,2)+'\n');
            console.log('Captured Search, Symbols and References product forms plus ordinary structured Session results.');
            await call('Browser.close');return;
        }
'''
marker="        if(process.argv.includes('--mcp-navigation')){"
assert marker in renderer
renderer=renderer.replace(marker,branch+marker,1)
(out/'render-review.cjs').write_text(renderer,encoding='utf-8')
print(out/'render-review.cjs')
