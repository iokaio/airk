# SPDX-License-Identifier: Apache-2.0
"""One fixed-budget native/local-model trial; original human gold stays in grader."""
import copy
import re
import time

from goal_rubric import grade
from grade_native_retrieval import evidence_recall


class GoalTrial:
    def __init__(self,client,sources,generate,core_request_template,arm,expected_profile=None):
        if arm not in ('ordinary_followup','deterministic_packet','arbiter'):raise ValueError('unknown frozen comparison arm')
        if core_request_template.get('schema')!='airk.arbiter.context.request/1':raise ValueError('native-owned Core request instance required')
        self.client,self.sources,self.generate=client,sources,generate
        self.template=copy.deepcopy(core_request_template);self.arm=arm;self.expected_profile=expected_profile
        if arm=='arbiter' and not expected_profile:raise ValueError('Arbiter trial requires exact admitted candidate profile identity')

    def run(self,task):
        start=time.monotonic();calls=[];turns=[];seen={};canonical_seconds=0.;canonical_calls=0;canonical_bytes=0;transport_bytes=0
        preprocessing_seconds=0.;inference_seconds=0.;failure=None;final=None;model_advice=[];unobserved_inner_work=False;source_error=None
        tool='search' if self.arm=='ordinary_followup' else 'answer'
        if tool=='search':arguments={'query':task['question'],'limit':8,'max_bytes':4096,'full_chunks':True,'response_mode':'full','include_status':True}
        else:
            arguments=copy.deepcopy(self.template);arguments['question']=task['question']
        visited=set()
        while len(turns)<8:
            if canonical_calls>=8 or max(canonical_bytes,transport_bytes)>=262144 or canonical_seconds>=30:
                failure='read_budget_exhausted';break
            request_key=(tool,repr(sorted(arguments.items())))
            if request_key in visited:failure='no_progress_repeated_read';break
            visited.add(request_key)
            if tool=='answer':arguments['budget']['deadline_ms']=max(1,int((30-canonical_seconds)*1000))
            receipt=self.client.call(tool,arguments,30-canonical_seconds);calls.append(receipt)
            preprocessing_seconds+=receipt.get('adapter_preprocessing_seconds',0.)
            canonical_seconds+=receipt['elapsed_seconds']
            transport_bytes+=receipt['stdout_bytes']
            value=receipt['full_canonical_value']
            if tool!='answer':
                canonical_calls+=1
                if receipt['envelope'] and isinstance(receipt['envelope'].get('content'),list) and receipt['envelope']['content']:
                    canonical_bytes+=len(receipt['envelope']['content'][0]['text'].encode())
            if receipt.get('validated') is not True or receipt.get('identity_error') or receipt.get('parse_error') or receipt['timed_out'] or receipt['returncode']!=0 or not isinstance(value,dict) or receipt['envelope'].get('isError'):
                unobserved_inner_work=tool=='answer'
                failure='native_call_failed';break
            if tool=='answer':
                work=value['work'];canonical_calls+=work['calls'];canonical_bytes+=work['bytes']
                if work['input_tokens']>2048:failure='native_internal_token_budget_exceeded';break
                if self.arm=='arbiter':
                    if not value['applications'] or any(application['availability']!='ready' or application['profile']!=self.expected_profile for application in value['applications']):
                        failure='arbiter_candidate_not_ready';break
                    model_advice.extend(value['advice'])
                elif value['advice'] or work['input_tokens']>0 or (work.get('inference_us') or 0)>0:
                    failure='deterministic_arm_used_model_work';break
            if not turns:
                initial=value if tool=='search' else next((record['canonical_result'] for record in value['records'] if record['operation']=='search' and record['arguments'].get('query')==task['question']),None)
                if not isinstance(initial,dict) or initial.get('execution',{}).get('served_mode')!='hybrid' or initial.get('execution',{}).get('incomplete') is not False:
                    failure='native_read_population_changed';break
            if canonical_calls>8 or max(canonical_bytes,transport_bytes)>262144 or canonical_seconds>30:
                failure='native_total_budget_exceeded';break
            before=time.monotonic()
            try:self.sources.observe(value)
            except (ValueError,KeyError,TypeError) as error:source_error=str(error);failure='source_provenance_failed';break
            finally:preprocessing_seconds+=time.monotonic()-before
            generation_start=time.monotonic();response=self.generate(task['question'],self.sources.visible_candidates())
            preprocessing_seconds+=max(0.,time.monotonic()-generation_start-response['elapsed_seconds'])
            inference_seconds+=response['elapsed_seconds'];turns.append(response)
            for record in response['shown_records']:seen.setdefault(record['id'],[]).append(record)
            if response['input_tokens']>4096 or response['output_tokens']>128:
                failure='external_model_token_budget_exceeded';break
            parsed=response['parsed']
            if parsed is None:failure='invalid_agent_json';break
            if not parsed['need_more']:final=parsed;break
            query=parsed['query'].strip()
            if not query:failure='empty_followup';break
            if query.startswith('READ '):
                match=re.fullmatch(r'READ (r[1-9][0-9]*)',query)
                if not match or match[1] not in seen:failure='unreturned_read_target';break
                tool,arguments=self.sources.read_followup(match[1])
            else:
                tool='search';arguments={'query':query,'limit':8,'max_bytes':4096,'full_chunks':True,'response_mode':'full','include_status':True}
        if final is None and failure is None:failure='external_model_turn_budget_exhausted'
        provided=[record for values in seen.values() for record in values];cited=[];citation_errors=0
        if final is not None:
            for identifier in final['citations']:
                if identifier not in seen:citation_errors+=1
                else:cited.extend(seen[identifier])
        grading=grade(task,final['answer'] if final else '',provided,cited)
        shown_recall,shown_complete=evidence_recall(task,provided)
        cited_recall,cited_complete=evidence_recall(task,cited)
        if failure or citation_errors:grading['completion']=False
        return {'task_id':task['id'],'arm':self.arm,'expected_profile':self.expected_profile,'success':grading['completion'],
            'final_response':final,
            'human_evidence':{'shown_recall_fraction':[shown_recall.numerator,shown_recall.denominator],
                'cited_recall_fraction':[cited_recall.numerator,cited_recall.denominator],
                'shown_complete_original_alternative':shown_complete,'cited_complete_original_alternative':cited_complete},
            'agent_final_citation_references':len(final['citations']) if final else 0,
            'agent_final_valid_citation_references':len(final['citations'])-citation_errors if final else 0,
            'grade':grading,'failure':failure,'citation_identity_errors':citation_errors,'external_model_turns':len(turns),
            'canonical_calls':None if unobserved_inner_work else canonical_calls,'canonical_calls_lower_bound':canonical_calls,'unobserved_native_inner_work':unobserved_inner_work,'native_rpc_attempts':len(calls),'source_error':source_error,'canonical_returned_bytes':None if unobserved_inner_work else canonical_bytes,'canonical_returned_bytes_lower_bound':canonical_bytes,'native_rpc_returned_bytes':transport_bytes,'canonical_seconds':canonical_seconds,
            'external_inference_seconds':inference_seconds,'preprocessing_seconds':preprocessing_seconds,'wall_seconds':time.monotonic()-start,
            'all_native_calls':calls,'all_model_turns':turns,'provided_source_ranges':provided,'cited_source_ranges':cited,'model_advice':model_advice,
            'scope':'Actual native/local-model comparison trial only. No gold feedback or hidden retries. Qualification requires complete frozen population, paired summaries and all source/profile/resource gates.'}
