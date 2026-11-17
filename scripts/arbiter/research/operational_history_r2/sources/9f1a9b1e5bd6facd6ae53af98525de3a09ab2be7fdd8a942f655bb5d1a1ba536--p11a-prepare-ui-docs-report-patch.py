# SPDX-License-Identifier: Apache-2.0
"""Inert Workbench consumer/docs delta after corrected P11a report+transport."""
import difflib,hashlib,json
from pathlib import Path
base=Path(__file__).resolve().parent;root=base/'p10-worktree';changes={}
def read(name):
    value=(root/name).read_text(encoding='utf-8',errors='surrogateescape');changes[name]=[value,value];return value
def replace(value,old,new):
    assert value.count(old)==1,old
    return value.replace(old,new)
name='packages/vscode/src/workbenchContracts.ts';text=read(name)
text=replace(text,"compact:'Compact results',include_count:","compact:'Compact results',include_profile_families:'Include effective profile families',include_count:");changes[name][1]=text
name='packages/vscode/src/workbenchRender.ts';text=read(name)
text=replace(text,'    readGuidance?:Value;','    readGuidance?:Value;\n    profileFamilies?:Value;')
text=replace(text,"        read_guidance:Object.keys(object(source.read_guidance)).length?source.read_guidance:undefined,","        read_guidance:Object.keys(object(source.read_guidance)).length?source.read_guidance:undefined,\n        profile_families:Object.keys(object(source.profile_families)).length?source.profile_families:undefined,")
text=replace(text,'            ...(metadata.read_guidance?{readGuidance:object(metadata.read_guidance)}:{}),','            ...(metadata.read_guidance?{readGuidance:object(metadata.read_guidance)}:{}),\n            ...(metadata.profile_families?{profileFamilies:object(metadata.profile_families)}:{}),')
text=replace(text,"'publication','read_guidance','scope_observation']","'publication','read_guidance','profile_families','scope_observation']");changes[name][1]=text
name='packages/vscode/src/workbenchWebview/main.tsx';text=read(name)
needle='''        {step.readGuidance&&<details><summary>Guidance vocabulary and limits</summary><pre>{canonical(step.readGuidance)}</pre></details>}'''
text=replace(text,needle,needle+'''
        {step.profileFamilies&&<details open><summary>Effective profile families (metadata only)</summary><p class="note">Support, retention eligibility and reply inclusion are separate observations. Unavailable collection settings and unknown pending state remain unknown. This report does not establish fact counts, completeness or live-source equality.</p><pre>{canonical(step.profileFamilies)}</pre></details>}''');changes[name][1]=text
name='packages/vscode/src/test/workbenchClientLifetime.test.ts';text=read(name)
text=replace(text,"    const stub={EventEmitter:Emitter,ViewColumn:","    const stub={env:{clipboard:{writeText:async(value:string)=>{exports.push(Buffer.from(value));}}},EventEmitter:Emitter,ViewColumn:")
text+='''
test('actual Workbench host forwards opt-in form plan and preserves complete report in copies and exports',async()=>{
    const report={schema:'airk.profile-families/1',file:{path:'api/policy.md',publication:'one-head',source_hash:'ab'.repeat(32)},applied_file:{adapter_id:'markdown',effective_profile_digest:'cd'.repeat(32)},configured_collection_policy:{state:'unavailable',reason:'selected_scope'},published_collection_policy:{state:'unavailable',reason:'selected_scope'},pending_collection_reindex:null,
        families:Array.from({length:18},(_,i)=>({family:`family-${i}`,support:'unknown',retention:'selected',reply_inclusion:'omitted'})),future_native_field:{complete:'keep-all-fields'}};
    const envelope={status:'complete',executed:true,publication:'one-head',calls:1,steps:[{id:'info',tool:'file_info',status:'complete',result:{read:{status:'content'},content:{path:'api/policy.md',profile_families:report}}}],evidence:[]};
    const f=fixture(true);
    try{
        f.respond(async tool=>({payload:tool==='airk_parse'?envelope:{}}));
        await f.state.select(A);
        await f.state.message({type:'run',input:'Inspect profile families',prepare:false,plan:{steps:[{id:'info',tool:'file_info',args:{path:'api/policy.md',include_profile_families:true}}]}});
        const call=f.created.flatMap(client=>client.requests).find(call=>call.tool==='airk_parse'&&object(call.args.plan).steps);
        assert.equal(object((object(call?.args.plan).steps as Value[])[0].args).include_profile_families,true);
        const turn=f.state.session!.turns.at(-1)!;
        assert.deepEqual(object(object((turn.envelope!.steps as Value[])[0].result).content).profile_families,report);
        for(const type of ['copy','export'])for(const format of ['json','markdown'])await f.state.message({type,turn:turn.id,format});
        assert.equal(f.exports.length,4);
        for(const output of f.exports){assert.ok(output.includes('keep-all-fields'));assert.ok(output.includes('family-17'));assert.ok(output.includes('selected_scope'));}
    }finally{f.provider.dispose();}
});
''';changes[name][1]=text
ui_test='''// SPDX-License-Identifier: Apache-2.0
//! Supplemental consumer checks; native transport producer parity is covered
//! by airk-cli/tests/combined_reads/profile_families.rs, not inferred here.
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import {object,toolFormArguments,toolFormFields,type WorkbenchTurn} from '../workbenchContracts';
import {discoveryDetails,renderExport} from '../workbenchRender';
import {newSession,SessionStore} from '../workbenchSessions';

const families=['saved_identity','saved_text','lexical_candidates','vectors','declarations','imports','written_calls','framework_structure','manifest_packages_dependencies','requirements_design_decisions','headings_sections','standalone_dates_bounds','tables_rows','mapped_regions','declaration_continuity','document_unit_continuity','imported_assertions','ocr_text_facts'];
function report(){return {schema:'airk.profile-families/1',basis:'metadata_only',file:{publication:'one-head',path:'src/a.rs',source_hash:'ab'.repeat(32)},applied_file:{adapter_id:'rust',adapter_version:'fixture',effective_profile_digest:'cd'.repeat(32),legacy_detail:'agent_lean'},configured_collection_policy:{state:'unavailable',reason:'selected_scope'},published_collection_policy:{state:'unavailable',reason:'selected_scope'},pending_collection_reindex:null,families:families.map(family=>({family,support:'unknown',retention:family==='manifest_packages_dependencies'?'not_retained':'selected',reply_inclusion:'omitted'})),future_native_field:{identity:'preserve opaque metadata'}};}
function turn():WorkbenchTurn{return {id:'families',at:1,input:'/info src/a.rs --include-profile-families',format:'structured',request:{plan:{steps:[{id:'info',tool:'file_info',args:{path:'src/a.rs',include_profile_families:true}}]}},envelope:{status:'complete',executed:true,publication:'one-head',steps:[{id:'info',tool:'file_info',status:'complete',result:{read:{status:'content'},content:{path:'src/a.rs',profile_families:report()}}}],evidence:[{step:'info',text:'selected evidence',publication:'one-head',citation:{path:'src/a.rs',start_line:1,end_line:1,content_hash:'ab'.repeat(32)}}]}};}

test('native-schema form keeps Default absent and explicit false/true typed',()=>{
    const schema={type:'object',required:['path'],properties:{path:{type:'string'},include_profile_families:{type:'boolean',default:false,description:'Native metadata-only family report'}}};
    const field=toolFormFields(schema).find(field=>field.name==='include_profile_families')!;
    assert.equal(field.type,'boolean');assert.equal(field.required,false);assert.equal(field.defaultValue,false);
    assert.deepEqual(toolFormArguments(schema,{path:'src/a.rs',include_profile_families:''}),{path:'src/a.rs'});
    for(const choice of ['false','true'])assert.equal(toolFormArguments(schema,{path:'src/a.rs',include_profile_families:choice}).include_profile_families,choice==='true');
    assert.throws(()=>toolFormArguments(schema,{path:'src/a.rs',include_profile_families:'yes'}));
});
test('structured details and full/selected JSON and Markdown preserve every family and unknown field',()=>{
    const entry=turn(),session=newSession('ab'.repeat(32));session.turns=[entry];const original=structuredClone(entry.envelope);
    assert.deepEqual(discoveryDetails(entry.envelope!)[0].profileFamilies,report());
    for(const format of ['json','markdown'] as const)for(const selected of [undefined,['record-0']]){
        const output=renderExport(session,format,entry.id,selected);
        for(const family of families)assert.ok(output.includes(family));
        assert.ok(output.includes('preserve opaque metadata'));assert.ok(output.includes('selected_scope'));assert.ok(output.includes('not_retained'));assert.ok(output.includes('omitted'));
        if(format==='json'&&selected){const coverage=JSON.parse(output).turns[0].envelope.source_coverage;assert.deepEqual(coverage[0].profile_families,report());}
    }
    assert.deepEqual(entry.envelope,original);
    const legacy=turn();delete object(object((legacy.envelope!.steps as Record<string,unknown>[])[0].result).content).profile_families;
    assert.equal(discoveryDetails(legacy.envelope!)[0].profileFamilies,undefined);
});
test('actual session save/load preserves opt-in draft and full report without rebuilding it',async()=>{
    const directory=await fs.mkdtemp(path.join(os.tmpdir(),'airk-family-session-'));
    try{
        const store=new SessionStore(directory),session=newSession('ab'.repeat(32));session.turns=[turn()];
        session.formDraft={activeTool:'file_info',values:{file_info:{path:'src/a.rs',include_profile_families:'true'}},revision:1,scope:'',recipe:''};
        await store.save(session,true);const restored=await store.load(session.project,session.id);
        assert.deepEqual(restored.turns[0].envelope,session.turns[0].envelope);assert.deepEqual(restored.formDraft,session.formDraft);
        for(const format of ['json','markdown'] as const)assert.ok(renderExport(restored,format).includes('preserve opaque metadata'));
    }finally{await fs.rm(directory,{recursive:true,force:true});}
});
'''
changes['packages/vscode/src/test/profileFamilies.test.ts']=['',ui_test]
doc='''# Per-file profile family reporting — P11a candidate

This is the **unaccepted P11a candidate** in the unpublished 1.8.8 working build.
It depends on sequential P10 acceptance. Implementation, native/CLI/MCP/Workbench
parity and frozen qualification must pass before acceptance; the existence of
this guide is not validation evidence. No new extraction profile is introduced.

Opt in on the existing file-information read:

```sh
airk info src/example.rs --include-profile-families --verification off
airk tools call airk_file_info --args '{"path":"src/example.rs","include_profile_families":true,"verification":"off"}'
```

MCP uses `include_profile_families: true` on `airk_file_info` or its public
`file_info` alias. Workbench's File info form uses **Include effective profile
families**: Default omits the input, No explicitly selects false, and Yes selects
true. Default/false retain the old result shape and evidence validator. A report
has its own conditional representation: a validator for old reduced FileInfo
cannot suppress a newly requested report. Existing revision, hash, origin and
follow-move checks still apply. Full/Terse response modes and Lean/Rich reply
choices preserve the complete report;
Rich does not recreate facts that were never retained.

Reporting itself uses one authenticated current reader and the requested file's
retained profile metadata. It does not reopen originals, rerun header selection,
walk the corpus, extract content, load models or invoke producers. Existing
[saved-source verification](source-verification.md) remains independent: the
examples explicitly select Off to request metadata only. A family report is not
a live-file equality, extraction-quality or completeness claim.

## Independent identities and observations

The report binds the exact file path, source hash and publication. Its applied
file `effective_profile_digest` comes from that retained `ExtractionProfile`,
including adapter ID/version and legacy detail. Configured/published collection
`policy_digest` values have a different meaning and different identity framing.
Saving a new collection policy does not reinterpret an old file profile.
Configured effective per-file selection remains unknown when fresh source or
header inputs would be required. Mixed indexes have no universal file adapter.

Authorized partial Project scopes still receive the file report. Collection
policy observations are explicitly `unavailable` with reason `selected_scope`;
`pending_collection_reindex: null` means unknown, not false. Private policy bytes
are not read in that branch. Real I/O/integrity failures retain normal errors.

Each of the following eighteen families independently reports adapter support
(`supported`, `unsupported`, `unknown`), retention eligibility (`retained`,
`selected`, `not_retained`, `unknown`) and actual operation payload inclusion
(`included`, `selected`, `omitted`, `unknown`):

| Family | Qualification |
| --- | --- |
| Saved identity | Retained file/profile/native-source identity, not live equality |
| Saved text | Eligibility does not promise complete extraction |
| Lexical candidates | Written token candidates, not resolved references |
| Vectors | Independent selection/model/pipeline metadata |
| Declarations | Exact versioned adapter support |
| Imports | Written imports, not universal resolution |
| Written calls | Syntax candidates, not resolved call graphs |
| Framework structure | Fine-family support can remain unknown |
| Manifest packages/dependencies | Legacy Agent Lean omits native package kinds |
| Requirements/design/decisions | Independent from standalone dates |
| Headings/sections | Adapter and original-location limits remain |
| Standalone dates/bounds | Legacy Agent Lean does not retain these kinds |
| Tables/rows | Legacy Agent Lean retains eligible table kinds |
| Mapped regions | Needs independent mapping metadata |
| Declaration continuity | Legacy Balanced/Detailed policy remains distinct |
| Document-unit continuity | Legacy Detailed eligibility differs from Balanced |
| Imported assertions | Independent producer/build/source provenance |
| OCR text/facts | Independent explicit OCR evidence and limitations |

Zero matches cannot establish unsupported/not-retained status. Fine support
without versioned registry evidence stays unknown; broad structural capability
flags do not certify every family. FileInfo returns metadata/counts rather than
the underlying facts/text/vectors, so those payload families are omitted even
when eligible for retention. Saved-identity final renderer inclusion may remain
unknown. A partial field selection is not a wholly omitted family.

Missing historical profile detail keeps its existing Detailed meaning. Legacy
Agent Lean, Balanced and Detailed profile IDs are preserved; Balanced declaration
continuity is not Detailed document-unit continuity. OCR/model adapters and their
dependencies, retained evidence and ranking remain unchanged.

## Workbench details, copies and persistence

Structured step details show the complete opaque native report, including unknown
future fields. JSON/Markdown copies and exports preserve it; selected exports
retain the qualifying metadata for their selected source steps and disclose the
subset. Session restoration preserves the original envelope and opt-in form
choice. Saved-result privacy settings may remove an entire result as documented;
they do not silently rewrite family rows.

Automatic Inspector/source-open/checker calls keep absent opt-in behavior. Use the
Workbench File info form for this candidate report. Strategy conversion refuses
unsupported native inputs and directs users to Save export rather than dropping
the flag or reducing the result.

These proposed checks supplement actual native transport tests; renderer mocks
alone do not establish CLI/MCP/Workbench parity. Full qualification and manual
acceptance remain pending in the [implementation ledger](research/implementation-v190.md).
'''
changes['docs/profile-family-reporting.md']=['',doc]
for name,link in [('README.md','docs/profile-family-reporting.md'),('docs/README.md','profile-family-reporting.md'),('packages/vscode/README.md','https://github.com/iokaio/airk/blob/main/docs/profile-family-reporting.md'),('docs/releases/release-1.8.8.md','../profile-family-reporting.md')]:
    text=read(name);heading,rest=text.split('\n',1)
    paragraph=f'\nP11a’s [per-file profile family reporting candidate]({link}) adds an explicit opt-in to File info. It keeps applied file-profile identity separate from collection policy, preserves scoped unknowns and retains complete native metadata through Workbench details and exports. P11a is **unaccepted**; sequential P10 acceptance and native/CLI/MCP/Workbench qualification remain prerequisites.\n'
    changes[name][1]=heading+'\n'+paragraph+rest
for name,link in [('docs/reply-detail.md','profile-family-reporting.md'),('docs/workbench/implementation.md','../profile-family-reporting.md')]:
    text=read(name);text+=f'\n## P11a candidate: explicit per-file family metadata\n\nThe unaccepted [family-reporting candidate]({link}) uses `include_profile_families` on existing FileInfo reads. Default/false retain the old representation and validator. Workbench’s schema-driven File info form offers an explicit choice; complete native report metadata survives structured details, JSON/Markdown copies/exports and session restore. Reporting adds no source reads or profile migration; existing verification remains independent. Full qualification and sequential phase acceptance remain pending.\n';changes[name][1]=text
patch=''.join(''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+name if old else '/dev/null',tofile='b/'+name)) for name,(old,new) in changes.items())
(base/'p11a-ui-docs-report-review.patch').write_text(patch,encoding='utf-8',errors='surrogateescape')
(base/'p11a-ui-docs-report-review-base.json').write_text(json.dumps({name:hashlib.sha256((root/name).read_bytes()).hexdigest() if old else None for name,(old,_new) in changes.items()},indent=2)+'\n',encoding='utf-8')
scratch=base/'p11a-ui-docs-report-proposed';scratch.mkdir(exist_ok=True)
for name,(_old,new) in changes.items():
    target=scratch/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(new,encoding='utf-8',errors='surrogateescape')
print('Prepared inert P11a UI/docs delta:',len(changes),'files; no source/compiler inputs changed')
