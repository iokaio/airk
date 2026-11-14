# SPDX-License-Identifier: Apache-2.0
"""Close six complete native goal conditions before computing frozen comparisons."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

from audit_sources import audit_pass
from goal_rubric import grade
from goal_summary import add_descriptive_intervals, summarize
from grade_native_retrieval import evidence_recall
from native_goal_client import require_normal_session_close
from summarize_native_calibration import MODELS

ROOT = Path(__file__).resolve().parents[2]
PROTOCOL_FILES = ('tools/retrieval-train/native_eval_session.cjs',
                  'packages/vscode/out/airkctl/session.js',
                  'packages/vscode/out/airkctl/runner.js',
                  'packages/vscode/out/airkctl/adminChunks.js')


def sealed_map(value,label,exact_keys=None):
    if not isinstance(value,dict) or not value or any(not isinstance(key,str) or not isinstance(pin,str) or len(pin)!=64 or any(char not in '0123456789abcdef' for char in pin) for key,pin in value.items()):
        raise ValueError('missing/invalid sealed identity: '+label)
    if exact_keys is not None and set(value)!=set(exact_keys):raise ValueError('sealed identity fields changed: '+label)
    return value


def verify_execution_closure(conditions,expected_protocol,ready=None):
    """Bind actual agent, native images and consumed I2 code across all calls."""
    agent_reference=None;runtime_reference=None;protocol_reference=None;metadata_reference=None
    for condition in conditions:
        agent=sealed_map(condition.get('agent_asset_seals'),'answering agent assets')
        if agent_reference is None:agent_reference=agent
        elif agent!=agent_reference:raise ValueError('conditions used different answering agent assets')
        for trial in condition['trials']:
            require_normal_session_close(trial.get('client_close'))
            calls=trial.get('all_native_calls')
            if not isinstance(calls,list) or not calls:raise ValueError('actual native calls missing')
            session_identity=None
            for call in calls:
                if call.get('validated') is not True or call.get('identity_error') or call.get('parse_error') or call.get('timed_out') or call.get('returncode')!=0:
                    raise ValueError('native call integrity failed before comparison closure')
                runtime=sealed_map(call.get('runtime_image_pins'),'matched native pair',('airk','airkctl'))
                if runtime_reference is None:runtime_reference=runtime
                elif runtime!=runtime_reference:raise ValueError('conditions/calls used different matched native images')
                exchange=call.get('session_exchange');identity=exchange.get('identity') if isinstance(exchange,dict) else None
                if not isinstance(identity,dict):raise ValueError('actual I2 identity missing')
                seals=sealed_map(identity.get('input_seals'),'actual I2 source/module inputs')
                normalized={key.replace('\\','/'):pin for key,pin in seals.items()}
                protocol={}
                for relative,expected in expected_protocol.items():
                    matched=[pin for key,pin in normalized.items() if key.endswith('/'+relative) or key==relative]
                    if matched!=[expected]:raise ValueError('consumed I2 helper/SDK source differs: '+relative)
                    protocol[relative]=matched[0]
                metadata=[pin for key,pin in normalized.items() if key.endswith('/airk-runtime.json')]
                if len(metadata)!=1:raise ValueError('actual matched-pair metadata seal missing or ambiguous')
                if protocol_reference is None:protocol_reference=protocol;metadata_reference=metadata[0]
                elif protocol!=protocol_reference or metadata[0]!=metadata_reference:raise ValueError('conditions/calls used different I2 protocol or runtime metadata')
                if identity.get('image',{}).get('sha256')!=runtime['airkctl']:raise ValueError('actual I2 child image differs from matched pair')
                if identity.get('index_id')!=call.get('index_id'):raise ValueError('actual I2 selected index differs from native call')
                if identity.get('ready_sha256')!=condition['native_fixture_ready_sha256'] or identity.get('bindings_sha256')!=condition['native_index_bindings_sha256']:
                    raise ValueError('actual I2 fixture/binding source differs')
                if ready is not None and (identity.get('service_instance')!=ready['health']['instance'] or identity.get('catalog_namespace')!=ready['health']['catalog_clock']['namespace']):
                    raise ValueError('actual I2 service instance/catalog differs from original Ready')
                if session_identity is None:session_identity=identity
                elif identity!=session_identity:raise ValueError('question changed its original I2 session identity')
            if trial['client_close'].get('identity')!=session_identity:raise ValueError('closed I2 child differs from actual invoked session')
    return {'agent_asset_seals':agent_reference,'runtime_image_pins':runtime_reference,
            'protocol_seals':protocol_reference,'runtime_metadata_sha256':metadata_reference}


def close_conditions(conditions, ready, expected_ids, pins):
    profiles = {row['manifest']: row['model'] for row in ready['profiles']}
    if len(ready['profiles']) != 4 or len(profiles) != 4 or set(profiles.values()) != MODELS:
        raise ValueError('exact four distinct native profile/model slots required')
    if len(conditions) != 6:raise ValueError('exact six complete conditions required')
    result = {}
    for condition in conditions:
        if condition.get('complete') is not True or condition.get('evaluation') is not True or condition.get('release_qualified') is not False or condition.get('split') != 'final' or 'campaign_error' in condition:
            raise ValueError('condition is partial, failed or not the frozen final population')
        for key, value in pins.items():
            if condition.get(key) != value:raise ValueError('condition frozen input binding changed: '+key)
        arm = condition['arm'];profile = condition['expected_profile']
        if arm == 'arbiter':
            if profile not in profiles:raise ValueError('condition profile is outside exact native Ready roster')
            name = profiles[profile]
        elif arm in ('ordinary_followup', 'deterministic_packet') and profile is None:name = arm
        else:raise ValueError('unknown comparison condition')
        rows = condition['trials']
        if name in result or len(rows) != 100 or {row['task_id'] for row in rows} != set(expected_ids) or any(row['arm'] != arm or row['expected_profile'] != profile for row in rows):
            raise ValueError('duplicate model/condition, incomplete IDs or mixed profile identity')
        result[name] = rows
    if set(result) != MODELS | {'ordinary_followup', 'deterministic_packet'}:
        raise ValueError('closed comparison roster differs')
    return result


def span_key(row):
    return tuple(row[key] for key in ('id', 'file', 'byte_start', 'byte_end', 'source_sha256', 'text'))


def verify_trial_grade(trial, task, fixture, source_files):
    provided = trial['provided_source_ranges'];shown = [record for turn in trial['all_model_turns'] for record in turn['shown_records']]
    if Counter(map(span_key, shown)) != Counter(map(span_key, provided)):
        raise ValueError('claimed provided source differs from actual shown model records')
    final = trial['final_response']
    if final is not None and (not trial['all_model_turns'] or final != trial['all_model_turns'][-1]['parsed'] or final['need_more'] is not False):
        raise ValueError('claimed final answer differs from actual stopping response')
    expected_cited = [row for identifier in (final['citations'] if final else []) for row in provided if row['id'] == identifier]
    if Counter(map(span_key, expected_cited)) != Counter(map(span_key, trial['cited_source_ranges'])):
        raise ValueError('claimed cited ranges differ from actual final record IDs')
    refs=len(final['citations']) if final else 0
    known={row['id'] for row in provided}
    errors=sum(identifier not in known for identifier in final['citations']) if final else 0
    if trial['citation_identity_errors']!=errors or trial['agent_final_citation_references']!=refs or trial['agent_final_valid_citation_references']!=refs-errors:
        raise ValueError('final agent citation denominators differ from actual response')
    for row in provided:
        source = source_files.get(row['file'])
        if not source:raise ValueError('foreign source in goal record')
        raw = (fixture/row['file']).read_bytes()
        if row['source_sha256'] != source['sha256'] or hashlib.sha256(raw).hexdigest() != source['sha256'] or type(row['byte_start']) is not int or type(row['byte_end']) is not int or not 0 <= row['byte_start'] < row['byte_end'] <= len(raw) or raw[row['byte_start']:row['byte_end']] != row['text'].encode():
            raise ValueError('provided model source bytes/ranges differ from approved source')
    recomputed = grade(task, final['answer'] if final else '', provided, trial['cited_source_ranges'])
    shown,shown_complete=evidence_recall(task,provided);cited,cited_complete=evidence_recall(task,trial['cited_source_ranges'])
    if trial['human_evidence']!={'shown_recall_fraction':[shown.numerator,shown.denominator],'cited_recall_fraction':[cited.numerator,cited.denominator],'shown_complete_original_alternative':shown_complete,'cited_complete_original_alternative':cited_complete}:
        raise ValueError('retained critical evidence recall differs from actual shown/cited ranges')
    if trial['failure'] or trial['citation_identity_errors']:recomputed['completion'] = False
    if recomputed != trial['grade'] or recomputed['completion'] is not trial['success']:
        raise ValueError('retained goal grading differs from original human rubric')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--condition',type=Path,action='append',required=True)
    for name in ('ready','rubric','question-scopes','request-template','fixture','output'):
        parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts') or len(args.condition)!=6 or len({path.resolve() for path in args.condition})!=6:
        parser.error('six distinct condition artifacts and fresh private output required')
    paths=[Path(__file__),Path(__file__).with_name('goal_summary.py'),Path(__file__).with_name('goal_rubric.py'),
           Path(__file__).with_name('goal-baseline-selection-freeze.json'),Path(__file__).with_name('goal-critical-evidence-freeze.json'),args.ready,args.rubric,
           args.question_scopes,args.request_template,args.fixture/'manifest.private.json',
           *[ROOT/name for name in PROTOCOL_FILES],*args.condition]
    retained={path:path.read_bytes() for path in paths};sha=lambda raw:hashlib.sha256(raw).hexdigest()
    pins={'native_fixture_ready_sha256':sha(retained[args.ready]),'rubric_sha256':sha(retained[args.rubric]),
          'question_scopes_sha256':sha(retained[args.question_scopes]),'request_template_sha256':sha(retained[args.request_template])}
    if pins['rubric_sha256']!='6004b1e1427661b236664352c9e1fdada5993d184685ca61bc13eba1a6b85423' or pins['question_scopes_sha256']!='4470916861293fe1185bb4c0469a8c051437f3f3451ba5c8a813a2c5df5f4a0c' or sha(retained[args.fixture/'manifest.private.json'])!='2627e54241d8e961cc03c005c02f1655be364462ada16bbafdc0434cf4192a60':
        raise ValueError('wrong fixed question/rubric/source identity')
    ready=json.loads(retained[args.ready]);rubric=json.loads(retained[args.rubric]);scopes=json.loads(retained[args.question_scopes]);manifest=json.loads(retained[args.fixture/'manifest.private.json'])
    expected_ids=[row['id'] for row in scopes['questions']];tasks={row['id']:row for row in rubric['tasks'] if row['split']=='test' and row['goal_selected']}
    if len(expected_ids)!=100 or set(expected_ids)!=set(tasks):raise ValueError('wrong fixed100 task set')
    conditions=[json.loads(retained[path]) for path in args.condition]
    closed=close_conditions(conditions,ready,expected_ids,pins)
    execution_identity=verify_execution_closure(conditions,{name:sha(retained[ROOT/name]) for name in PROTOCOL_FILES},ready)
    before=audit_pass(args.fixture,manifest);files={row['file']:row for row in manifest['files']}
    binding_bytes=[];algorithm_pins=[]
    for path,condition in zip(args.condition,conditions):
        if condition['source_before']['closed_roster']!=before or condition['source_after']['closed_roster']!=before:
            raise ValueError('condition original before/after source closure differs')
        binding=condition['native_index_bindings_sha256'];snapshot=path.parent/'source-snapshots'/binding;raw=snapshot.read_bytes()
        if sha(raw)!=binding:raise ValueError('actual native index binding snapshot changed')
        retained[snapshot]=raw;binding_bytes.append(json.loads(raw)['bindings'])
        code={Path(name).name:pin for name,pin in condition['input_seals'].items() if name.endswith('.py') or name.endswith('freeze.json') or name.endswith('policy.json')}
        algorithm_pins.append(code)
        if not any(Path(name).name=='goal_rubric.py' and pin==sha(retained[Path(__file__).with_name('goal_rubric.py')]) for name,pin in condition['input_seals'].items()):raise ValueError('condition human grading implementation changed')
        for trial in condition['trials']:verify_trial_grade(trial,tasks[trial['task_id']],args.fixture,files)
    if any(value!=binding_bytes[0] for value in binding_bytes[1:]) or any(value!=algorithm_pins[0] for value in algorithm_pins[1:]):
        raise ValueError('conditions used different index/source bindings or algorithms')
    result=add_descriptive_intervals(summarize(closed,expected_ids,sorted(MODELS)),closed,list(tasks.values()))
    critical=result['critical_evidence_comparisons']
    result['a6_goal_comparison_pass']=result['all_optional_completion_floors_pass'] and result['all_candidate_completion_comparisons_pass'] and result['primary_turn_floor_pass'] and len(critical)==4 and all(row['shown_critical_evidence_floor_pass'] and row['cited_critical_evidence_floor_pass'] and row['agent_citation_precision_no_regression'] is True for row in critical)
    result.update(schema='airk.private-native-goal-comparison/1',evaluation=True,release_qualified=False,
                  actual_execution_identity=execution_identity,
                  input_seals={str(path):sha(raw) for path,raw in retained.items()},
                  scope='Actual six-condition fixed100 public-human-rubric comparison only. Original failures/all turns and both paired controls retained; strongest baseline selection uses baseline outcomes only. Optional-only .75 floor; no new baseline acceptance floor. Source/resource/cancel/critical-evidence/noninterference/platform/package/release qualification remain separately required.')
    after=audit_pass(args.fixture,manifest)
    if after!=before or any(path.read_bytes()!=raw for path,raw in retained.items()):raise ValueError('goal summary input/source drift')
    output.mkdir(parents=True);(output/'comparison.private.json').write_text(json.dumps(result,indent=2)+'\n');(output/'runner.py').write_bytes(retained[Path(__file__)])
    print(json.dumps({key:value for key,value in result.items() if key not in ('input_seals','scope','conditions','paired_common_success')}))


if __name__=='__main__':main()
