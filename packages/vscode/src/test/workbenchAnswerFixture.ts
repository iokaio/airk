// SPDX-License-Identifier: Apache-2.0
import type { Value } from '../workbenchContracts';
export const answerRecord='31'.repeat(32), answerIndex='idx_'+'32'.repeat(16), answerProject='prj_'+'33'.repeat(16), answerRoot='src_'+'34'.repeat(16), answerHash='35'.repeat(32);
export const answerPublication={collection:'fixture',token:`airk-r1:${'36'.repeat(16)}:4:${'37'.repeat(32)}`};
export function answerSourceFixture(): {answer: Value; info: Value} {
    const verification:Value={schema:'airk.saved-source-verification/1',method:'sha256-full-saved-source/1',
        sources:[{id:'source-0',path:'api/source.rs',expected_hash:answerHash,source_root_id:answerRoot,source_relative_path:'api/source.rs',origin_id:'origin-0',reference_ids:['ref-0']}],
        origins:[{id:'origin-0',index_id:answerIndex,project_ids:[answerProject],publication:answerPublication.token}],
        references:[{id:'ref-0',source_id:'source-0',result_pointer:'/hits/0/citation'}]};
    const body:Value={collection:'fixture',publication:answerPublication,scope_observation:{fingerprint:'38'.repeat(32),members:[{index_id:answerIndex,source_root_ids:[answerRoot],memberships:[{project_id:answerProject}]}]},
        hits:[{citation:{path:'api/source.rs',content_hash:answerHash,start_line:7,end_line:9,location:{Lines:{start:7,end:9}}},text:'Canonical source bytes.',future_hit:{preserve:true}}],source_verification:verification,future_result:{preserve:true}};
    return {answer:{schema:'airk.arbiter.context/2',assessment:{scope:{index_id:answerIndex,authorization:'38'.repeat(32),publication:answerPublication},termination:'no_progress',coverage:{status:'unassessed',reason:'disabled'}},assessment_history:[],gaps:[],records:[{id:answerRecord,operation:'search',role:'unknown',scope:{index_id:answerIndex,authorization:'38'.repeat(32),publication:answerPublication},arguments:{query:'original question'},canonical_result:body}],paragraphs:[{text:'Cited paragraph',evidence:[answerRecord],interpretation:false}],future_packet:{preserve:true}},
        info:{collection:'fixture',publication:answerPublication,path:'api/source.rs',content_hash:answerHash,line_basis:'source',source:{root:'/native/verified-original',relative_path:'api/source.rs'},source_verification:structuredClone(verification)}};
}
