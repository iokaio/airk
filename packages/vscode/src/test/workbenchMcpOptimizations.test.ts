// SPDX-License-Identifier: Apache-2.0
/** Independent tests for typed native forms and new discovery metadata in Workbench. */
import assert from 'node:assert/strict';
import test from 'node:test';
import { parseWorkbenchMessage, toolFormArguments, toolFormFields, Value, WorkbenchSession, WorkbenchTurn } from '../workbenchContracts';
import { continuationCursor, continuationRequiredRecordBytes, continuations, discoveryDetails, discoveryFollowupPlan, markdown, renderExport } from '../workbenchRender';

const schema:Value={type:'object',required:['name'],properties:{
    name:{type:'string',description:'Exact identifier'},
    name_match:{type:'string',enum:['exact','suffix','contains']},
    include_count:{type:'boolean'},
    include_files:{type:'boolean'},
    limit:{type:'integer',minimum:1,maximum:100},
    language:{type:'string'},
}};

test('change selector forms preserve typed JSON arrays and reject malformed input',()=>{
    const changes={type:'object',properties:{selectors:{type:'array',maxItems:64,items:{type:'object'}}}};
    assert.equal(toolFormFields(changes)[0].type,'json');
    assert.equal(toolFormFields(changes)[0].maxLength,32768);
    assert.deepEqual(toolFormArguments(changes,{selectors:'[{"kind":"path","path":"queue.rs"}]'}),{selectors:[{kind:'path',path:'queue.rs'}]});
    const selectors=Array(8).fill({kind:'path',path:'a'.repeat(700)});
    assert.deepEqual(toolFormArguments(changes,{selectors:JSON.stringify(selectors)}),{selectors});
    for(const selectors of ['{','{}',JSON.stringify(Array(65).fill({kind:'path',path:'queue.rs'}))])assert.throws(()=>toolFormArguments(changes,{selectors}));
});

test('full Markdown exports retain every native field even when evidence cards are present',()=>{
    const turn:WorkbenchTurn={id:'full',at:1,input:'/search Queue',format:'structured',envelope:{status:'complete',steps:[{id:'query',tool:'search',status:'complete',result:{new_native_field:{retained:true},query_concurrency:32}}],evidence:[{step:'query',citation:{path:'queue.rs',start_line:1,end_line:1,content_hash:'fixture'},text:'struct Queue;'}]}};
    const output=markdown(turn);
    assert.match(output,/Complete response/);
    assert.match(output,/new_native_field/);
    assert.match(output,/"query_concurrency": 32/);
});

test('native forms keep optional booleans and enums typed without injecting defaults',()=>{
    const before=structuredClone(schema);
    const fields=toolFormFields(schema);
    assert.equal(fields.find(field=>field.name==='include_count')?.type,'boolean');
    assert.deepEqual(fields.find(field=>field.name==='name_match')?.values,['exact','suffix','contains']);
    assert.equal(fields.find(field=>field.name==='name')?.required,true);
    const args=toolFormArguments(schema,{name:'Session',name_match:'suffix',include_count:'true',include_files:'false',limit:'12',language:''});
    assert.deepEqual(args,{name:'Session',name_match:'suffix',include_count:true,include_files:false,limit:12});
    assert.deepEqual(toolFormArguments(schema,{name:'Session',name_match:'',include_count:'',include_files:'',limit:''}),{name:'Session'});
    assert.deepEqual(schema,before,'building fields/arguments cannot rewrite the native catalog');
});

test('invalid native field values fail before crossing the host boundary',()=>{
    const invalids:Record<string,string>[]=[
        {name:'Session',include_count:'yes'},
        {name:'Session',name_match:'fuzzy'},
        {name:'Session',limit:'1.5'},
        {name:'Session',limit:'NaN'},
        {name:'Session',limit:'101'},
        {name:''},
    ];
    for(const invalid of invalids){assert.throws(()=>toolFormArguments(schema,invalid));}
});

test('native nullable enum definitions and booleans are exposed by the actual form helpers',()=>{
    const native={type:'object',$defs:{NameMatch:{type:'string',enum:['exact','suffix','contains']}},properties:{
        name_match:{anyOf:[{$ref:'#/$defs/NameMatch'},{type:'null'}]},
        include_count:{type:['boolean','null']},
    }};
    assert.deepEqual(toolFormArguments(native,{name_match:'contains',include_count:'false'}),{name_match:'contains',include_count:false});
    assert.deepEqual(toolFormArguments(native,{name_match:'',include_count:''}),{});
});

test('reply detail uses an optional enum and survives plans and full exports',()=>{
    const native={type:'object',properties:{reply_detail:{type:['string','null'],enum:['lean','rich',null]}}};
    const fields=toolFormFields(native);assert.equal(fields[0].label,'Reply detail');assert.deepEqual(fields[0].values,['lean','rich']);
    assert.deepEqual(toolFormArguments(native,{reply_detail:''}),{});
    for(const choice of ['lean','rich']){
        const args=toolFormArguments(native,{reply_detail:choice});assert.deepEqual(args,{reply_detail:choice});
        const message={type:'run',input:'Selected reply',prepare:false,plan:{steps:[{id:'query',tool:'symbols',args}]}};
        assert.deepEqual(parseWorkbenchMessage(message),message);
        const session:WorkbenchSession={schema:1,id:'reply',project:'ab'.repeat(32),title:'Reply',createdAt:0,updatedAt:0,revision:0,omitted:0,turns:[{id:'t',at:0,input:'Reply',format:'structured',request:message,envelope:{reply_detail:choice,plan:message.plan,
            steps:[{id:'query',tool:'symbols',status:'complete',arguments:args,result:{reply_detail:choice}}],
            evidence:[{step:'query',text:'Queue',citation:{path:'queue.rs',start_line:1,end_line:1,content_hash:'ab'.repeat(32),location:{Lines:{start:1,end:1}}}}]}}]};
        assert.equal(JSON.parse(renderExport(session,'json')).turns[0].envelope.reply_detail,choice);
        assert.equal(discoveryDetails(session.turns[0].envelope!)[0].replyDetail,choice);
        assert.match(renderExport(session,'markdown'),new RegExp(`"reply_detail": "${choice}"`));
        assert.match(renderExport(session,'markdown','t',['record-0']),new RegExp(`"reply_detail": "${choice}"`));
    }
    assert.throws(()=>toolFormArguments(native,{reply_detail:'detailed'}));
});

test('typed discovery options survive Workbench run-message validation',()=>{
    const args=toolFormArguments(schema,{name:'Session',name_match:'contains',include_count:'false',limit:'20'});
    const message={type:'run',input:'symbols form',prepare:true,pathPrefix:'src/',
        plan:{path_prefix:'src/',steps:[{id:'form',tool:'symbols',args}]}};
    const accepted=parseWorkbenchMessage(message);
    assert.deepEqual(accepted,message);
    assert.equal(accepted?.type,'run');
    assert.ok(accepted?.type==='run'&&accepted.plan?.steps);
    assert.equal(args.include_count,false,'explicit false must not become the string false or disappear');
});

test('continuation discovers flattened fact and nested reference pages',()=>{
    const cases=[
        [{cursor:'facts-page'},'facts-page'],
        [{content:{cursor:'wrapped-page'}},'wrapped-page'],
        [{page:{cursor:'reference-page'}},'reference-page'],
        [{evidence:{page:{cursor:'nested-reference-page'}}},'nested-reference-page'],
    ] as const;
    for(const [result,cursor] of cases){
        assert.equal(continuationCursor(result),cursor);
        const envelope={plan:{budgets:{calls:12}},calls:3,steps:[{status:'complete',tool:'references',result}]};
        assert.deepEqual(continuations(envelope),[{id:'page-0',tool:'references',callsRemaining:9}]);
        assert.deepEqual(continuations({...envelope,calls:11}),[],'two native calls are required for guarded continuation');
    }
    for(const result of [{page:{cursor:42}},{page:{cursor:''}},{page:{more:true}},{}]){
        assert.equal(continuationCursor(result),undefined);
    }
});

test('Markdown keeps count precision, file summaries and qualified targets beside evidence',()=>{
    const turn:WorkbenchTurn={id:'one',at:1,input:'Inspect Session',format:'structured',envelope:{
        publication:'exact-publication',status:'complete',
        plan:{path_prefix:'src/',steps:[{id:'refs',tool:'references'}]},
        steps:[{id:'refs',tool:'references',status:'complete',result:{page:{
            more:true,cursor:'exact-cursor',count_unit:'matching_lines',files:[{path:'src/session.rs',count:2}],
            match_count:{total_matches:null,lower_bound:2,complete:false,scope:'scanned_candidates'},
        }}},{id:'links',tool:'relationships',status:'complete',result:{
            suggested_targets:[{target:'Session::spawn',resolution:'syntax_candidate',path:'src/session.rs'}],
            coverage:'Written-target candidates; not resolved references.',
        }}],
        evidence:[{step:'refs',text:'Session::spawn(7)',citation:{path:'src/session.rs',start_line:3,end_line:3,
            content_hash:'ab'.repeat(32),location:{Lines:{start:3,end:3}}}}],
    }};
    const before=structuredClone(turn.envelope);
    const output=markdown(turn);
    assert.match(output,/Session::spawn/);
    assert.match(output,/lower_bound|at least|atleast/i);
    assert.match(output,/matching_lines|matching lines/i);
    assert.match(output,/scanned_candidates|scanned candidates/i);
    assert.match(output,/syntax_candidate|syntax candidate/i);
    assert.deepEqual(turn.envelope,before,'rendering/export cannot mutate native continuation or evidence');
});

test('discovery display preserves unknown totals and bounds summaries without changing native results',()=>{
    const files=Array.from({length:105},(_,i)=>({path:`src/file-${i}.rs`,count:i}));
    const envelope={steps:[{id:'symbols',tool:'symbols',result:{match_count:{total_matches:null,lower_bound:7,complete:false,scope:'scanned_candidates'}}},
        {id:'refs',tool:'references',result:{page:{files,files_truncated:true,more:true,incomplete_reason:'scan_limit'}}}]};
    const before=structuredClone(envelope),details=discoveryDetails(envelope);
    assert.equal(details[0].count?.total_matches,null);
    assert.equal(details[0].count?.lower_bound,7);
    assert.equal(details[1].files.length,100);
    assert.equal(details[1].omittedFiles,5);
    assert.equal(details[1].filesTruncated,true);
    assert.equal(details[1].incompleteReason,'scan_limit');
    assert.deepEqual(envelope,before);
});

test('explicit discovery follow-up accepts only retained targets and exact definition pointers',()=>{
    const envelope={publication:'original-publication',executed:true,status:'partial',plan:{collection:'fictional',path_prefix:'src/'},steps:[
        {id:'links',tool:'relationships',status:'complete',arguments:{path_prefix:'src/narrow/'},result:{publication:{token:'original-publication'},suggested_targets:[{target:'Session::spawn'}]}},
        {id:'find',tool:'search',status:'complete',arguments:{},result:{publication:{token:'original-publication'},definitions:[{name:'Session',matches:[{symbol_id:'syntax:session',citation:{path:'src/session.rs'}}]}]}},
    ]};
    const relationship=discoveryFollowupPlan(envelope,'links','Session::spawn');
    assert.equal(relationship.publication,'original-publication');
    assert.equal((relationship.steps as {args:Value}[])[0].args.symbol,'Session::spawn');
    assert.equal(relationship.path_prefix,'src/narrow/');
    assert.equal((relationship.steps as {args:Value}[])[0].args.kind,'call');
    const definition=discoveryFollowupPlan(envelope,'find','Session','src/session.rs');
    assert.deepEqual((definition.steps as {args:Value}[])[0].args,{name:'syntax:session',path:'src/session.rs'});
    assert.throws(()=>discoveryFollowupPlan(envelope,'links','Invented::target'));
    assert.throws(()=>discoveryFollowupPlan(envelope,'find','Session','src/other.rs'));
    assert.throws(()=>discoveryFollowupPlan(envelope,'missing','Session::spawn'));
    assert.throws(()=>discoveryFollowupPlan({...envelope,publication:undefined},'links','Session::spawn'));
    assert.throws(()=>discoveryFollowupPlan({...envelope,executed:false},'links','Session::spawn'));
    assert.throws(()=>discoveryFollowupPlan({...envelope,status:'failed'},'links','Session::spawn'));
    for(const change of [
        {status:'failed'},
        {tool:'symbols'},
        {arguments:{path_prefix:'unrelated/'}},
        {result:{...envelope.steps[0].result,publication:{token:'different-publication'}}},
    ]){
        assert.throws(()=>discoveryFollowupPlan({...envelope,steps:[{...envelope.steps[0],...change}]},'links','Session::spawn'));
    }
    const missingId={...envelope,steps:[{...envelope.steps[1],result:{publication:'original-publication',definitions:[{name:'Session',matches:[{citation:{path:'src/session.rs'}}]}]}}]};
    assert.throws(()=>discoveryFollowupPlan(missingId,'find','Session','src/session.rs'));
    assert.ok(parseWorkbenchMessage({type:'followTarget',turn:'one',step:'links',target:'Session::spawn'}));
    assert.equal(parseWorkbenchMessage({type:'followTarget',turn:'one',step:'links',target:'Session::spawn',command:'shell'}),undefined);
    assert.equal(parseWorkbenchMessage({type:'followDefinition',turn:'one',step:'find',name:'Session',path:'src/session.rs',execute:true}),undefined);
});

test('native catalog lengths permit opaque cursors while rejecting overlong languages',()=>{
    const native={properties:{cursor:{type:'string',minLength:1,maxLength:32768},language:{type:'string',maxLength:128}}};
    assert.equal(toolFormFields(native).find(field=>field.name==='cursor')?.maxLength,32768);
    assert.equal(String(toolFormArguments(native,{cursor:'a'.repeat(8192)}).cursor).length,8192);
    assert.throws(()=>toolFormArguments(native,{cursor:'a'.repeat(32769)}));
    assert.throws(()=>toolFormArguments(native,{language:'a'.repeat(129)}));
});

test('empty resumable pages disclose required bytes while terminal omissions do not invent continuation',()=>{
    const resumable={page:{cursor:'same-boundary',more:true,required_record_bytes:9000}};
    assert.equal(continuationRequiredRecordBytes(resumable),9000);
    const plan={budgets:{calls:12}},calls=3;
    assert.deepEqual(continuations({plan,calls,steps:[{tool:'references',status:'complete',result:resumable}]}),
        [{id:'page-0',tool:'references',callsRemaining:9,requiredRecordBytes:9000}]);
    const terminal={evidence:{truncated:true},page:{more:false,incomplete_reason:'candidate_limit'}};
    assert.deepEqual(continuations({plan,calls,steps:[{tool:'references',status:'complete',result:terminal}]}),[]);
});

test('terminal truncated reference metadata remains explicit in Markdown and native JSON',()=>{
    const turn:WorkbenchTurn={id:'terminal',at:1,input:'Find uses',format:'structured',envelope:{status:'partial',publication:'original',
        evidence:[{text:'Session',citation:{path:'src/session.rs',start_line:1,end_line:1,content_hash:'ab'.repeat(32)}}],
        steps:[{tool:'references',status:'complete',result:{evidence:{truncated:true,coverage:'candidate scan incomplete',resolution:'textual'},
            page:{more:false,incomplete_reason:'candidate_limit'}}}],
    }};
    const original=JSON.stringify(turn.envelope),output=markdown(turn);
    assert.match(output,/"truncated": true/);
    assert.match(output,/candidate_limit/);
    assert.match(output,/candidate scan incomplete/);
    assert.match(output,/textual/);
    assert.equal(JSON.stringify(turn.envelope),original);
});

test('exact candidate counts retain unknown whole-query and token-boundary coverage even at zero',()=>{
    for(const count of [0,2]){
        const coverage={basis:'verified_lexical_candidates',candidate_scan_complete:true,query_complete:false,
            query_total_matches:null,token_boundary_complete:false,line_completeness:count===0?'no_returned_lines':'complete_source_lines'};
        const envelope={steps:[{id:'refs',tool:'references',status:'complete',result:{evidence:{truncated:true},page:{more:false,
            count_unit:'matching_lines',coverage,match_count:{total_matches:count,lower_bound:count,complete:true,scope:'verified_lexical_candidates'}}}}]};
        const details=discoveryDetails(envelope)[0];
        assert.equal(details.count?.total_matches,count);
        assert.equal(details.count?.scope,'verified_lexical_candidates');
        assert.equal(details.coverage?.query_complete,false);
        assert.equal(details.coverage?.query_total_matches,null);
        assert.equal(details.coverage?.token_boundary_complete,false);
        assert.equal(details.countUnit,'matching_lines');
        assert.equal(continuationCursor((envelope.steps[0]).result),undefined);
        assert.match(markdown({id:'t',at:1,input:'Session uses',format:'structured',envelope}),/"query_complete": false/);
    }
});

test('fragment results retain qualified unit and partial coverage without inventing a next page',()=>{
    const envelope={plan:{budgets:{calls:12}},calls:2,steps:[{id:'refs',tool:'references',status:'complete',result:{
        evidence:{truncated:true,hits:[{truncated:true,citation:{path:'docs/example.pdf',start_line:1,end_line:1,content_hash:'ab'.repeat(32)}}]},
        page:{more:false,incomplete_reason:'original_line_completeness_unverified',count_unit:'matching_line_fragments',
            coverage:{basis:'verified_lexical_candidates',candidate_scan_complete:true,query_complete:false,query_total_matches:null,
                token_boundary_complete:false,line_completeness:'includes_unverified_fragments'},
            match_count:{total_matches:null,lower_bound:1,complete:false,scope:'verified_lexical_candidates'}},
    }}]};
    const before=JSON.stringify(envelope),details=discoveryDetails(envelope)[0];
    assert.equal(details.countUnit,'matching_line_fragments');
    assert.equal(details.coverage?.line_completeness,'includes_unverified_fragments');
    assert.equal(details.count?.total_matches,null);
    assert.deepEqual(continuations(envelope),[]);
    const output=markdown({id:'fragment',at:1,input:'Session uses',format:'structured',envelope});
    assert.match(output,/matching_line_fragments/);
    assert.match(output,/includes_unverified_fragments/);
    assert.match(output,/"query_total_matches": null/);
    assert.equal(JSON.stringify(envelope),before);
});

test('selected exports retain only their source uncertainty and omit unrelated metadata and counts',()=>{
    for(const queryTotal of [null,77]){
        const selectedCitation={path:'src/chosen.rs',start_line:4,end_line:4,content_hash:'ab'.repeat(32),location:{Lines:{start:4,end:4}}};
        const turn:WorkbenchTurn={id:'selection',at:1,input:'Inspect Session',format:'structured',envelope:{publication:'original-publication',status:'partial',steps:[
            {id:'chosen',tool:'references',arguments:{private:'argument-secret'},result:{evidence:{truncated:true,resolution:'textual'},page:{
                count_unit:'matching_line_fragments',coverage:{basis:'verified_lexical_candidates',candidate_scan_complete:true,query_complete:false,
                    token_boundary_complete:false,line_completeness:'includes_unverified_fragments',query_total_matches:queryTotal,
                    private:'coverage-secret',lower_bound:999},
                match_count:{total_matches:77,lower_bound:77,complete:true,scope:'verified_lexical_candidates'},
                files:[{path:'private-file-secret',count:88}],definitions:[{name:'definition-secret'}],suggested_targets:[{target:'target-secret'}],
            }}},
            {id:'other',tool:'search',result:{coverage:'other-step-secret',hits:[{text:'other-result-secret'}]}},
        ],evidence:[
            {step:'chosen',publication:'original-publication',text:'selected source',citation:selectedCitation},
            {step:'other',publication:'original-publication',text:'unselected-source-secret',citation:{...selectedCitation,path:'src/other.rs'}},
        ]}};
        const session:WorkbenchSession={schema:1,id:'session',project:'ab'.repeat(32),title:'Selected evidence',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[turn]};
        const fullBefore=renderExport(session,'json'),original=structuredClone(turn.envelope);
        const selected=renderExport(session,'json',undefined,['record-0']);
        const value=JSON.parse(selected),envelope=value.turns[0].envelope;
        assert.equal(envelope.evidence.length,1);
        assert.deepEqual(envelope.evidence[0].citation,selectedCitation);
        assert.equal(envelope.publication,'original-publication');
        assert.deepEqual(envelope.steps,[]);
        assert.equal(envelope.source_coverage.length,1);
        const qualification=envelope.source_coverage[0];
        assert.equal(qualification.step,'chosen');
        assert.equal(qualification.scope,'original_source_request');
        assert.equal(qualification.truncated,true);
        assert.equal(qualification.count_unit,'matching_line_fragments');
        assert.equal(qualification.coverage.query_complete,false);
        assert.equal(qualification.coverage.token_boundary_complete,false);
        assert.equal(Object.hasOwn(qualification.coverage,'query_total_matches'),queryTotal===null);
        const selectedMarkdown=renderExport(session,'markdown',undefined,['record-0']);
        assert.match(selectedMarkdown,/Original source request coverage for selected evidence/);
        assert.match(selectedMarkdown,/includes_unverified_fragments/);
        for(const secret of ['argument-secret','coverage-secret','private-file-secret','definition-secret','target-secret','other-step-secret','other-result-secret','unselected-source-secret','"total_matches"','"lower_bound"']){
            assert.equal(selected.includes(secret),false,`JSON selection must omit ${secret}`);
            assert.equal(selectedMarkdown.includes(secret),false,`Markdown selection must omit ${secret}`);
        }
        assert.deepEqual(turn.envelope,original);
        assert.equal(renderExport(session,'json'),fullBefore,'selected export cannot mutate the full authoritative result');
    }
});
