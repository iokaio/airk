# SPDX-License-Identifier: Apache-2.0
"""Run one frozen comparison condition through actual resident native handlers.

Index bindings and the Core request instance must come from native setup/emission.
This isolated developer driver never creates a service, index ID or model readiness.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import time

from aggregate_native_retrieval import scope_key, valid_digest
from audit_sources import audit_pass
from eval_agent_smoke import generate_answer, load_fixed_agent
from goal_sources import FrozenSourceMetadata,SourceLedger
from goal_trial import GoalTrial
from native_goal_client import NativeGoalClient,require_normal_session_close
import qwen_export

ROOT = Path(__file__).resolve().parents[5]
INFRA_FAILURES = {'native_call_failed', 'source_provenance_failed', 'arbiter_candidate_not_ready',
                  'selected_stage_contract_failed','unobserved_native_model_work','native_read_population_changed'}


def original_task_metadata(task,split):
    return {'dataset':task['dataset'],'original_question_categories':task['original_question_categories'],
            'original_human_disagreement':task['disagreement'],
            'source_component':task['bootstrap_source_component'] if split=='confirmation' else None}


def guard_output_location(output, fixture, native_rows):
    """Observe actual directory ancestors before any evaluator metadata write."""
    target=Path(output).resolve();artifact_root=(ROOT/'artifacts').resolve(strict=True)
    sources=[Path(fixture).resolve(strict=True),*[Path(row['source_root']).resolve(strict=True) for row in native_rows]]
    def under(source):
        for parent in (target,*target.parents):
            try:
                parent.stat()
                if parent.samefile(source):return True
            except FileNotFoundError:continue
            except OSError as error:raise ValueError('output ancestor identity is unknown') from error
        return False
    try:
        target.lstat()
    except FileNotFoundError:pass
    else:raise ValueError('fresh private output required')
    if not under(artifact_root) or any(under(source) for source in sources):
        raise ValueError('evaluation metadata must stay outside all frozen fixture/source roots')
    return target


def main():
    started = time.monotonic();started_utc = datetime.now(timezone.utc).isoformat()
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runtime-pair', 'control-root', 'fixture-ready', 'index-bindings', 'request-template',
                 'question-scopes', 'rubric', 'fixture', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--assets', type=Path, default=ROOT/'artifacts/advanced-split/models/candidate-assets')
    parser.add_argument('--split', choices=('confirmation',), required=True)
    parser.add_argument('--campaign-freeze',type=Path,required=True)
    parser.add_argument('--campaign-freeze-sha256',required=True)
    parser.add_argument('--focus-profile')
    parser.add_argument('--arm', choices=('ordinary_followup', 'deterministic_packet', 'arbiter'), required=True)
    parser.add_argument('--profile-manifest', help='Exact native Core Profile manifest digest, not the human-readable profile_id')
    args = parser.parse_args();output = args.output.resolve()
    freeze_raw=args.campaign_freeze.read_bytes()
    if hashlib.sha256(freeze_raw).hexdigest()!=args.campaign_freeze_sha256:raise ValueError('selected A6 freeze changed')
    freeze=json.loads(freeze_raw)
    if freeze.get('schema')!='airk.private-selected-native-goal-freeze/1' or freeze.get('status')!='FROZEN_BEFORE_SELECTED_GOAL_OUTCOMES' or freeze.get('approved') is not True:raise ValueError('reviewed preoutcome selected A6 freeze required')
    expected_pins=freeze['input_pins']
    for path,pin in freeze['source_pins'].items():
        if qwen_export.digest(path)!=pin:raise ValueError('selected A6 source closure changed')
    if args.focus_profile!=freeze['shared_Focus_profile']:raise ValueError('frozen common Focus condition differs')
    for name in ('runtime_pair','control_root','fixture_ready','index_bindings','request_template','question_scopes','rubric','fixture','assets'):
        if str(getattr(args,name).resolve())!=freeze['paths'][name]:raise ValueError('foreign frozen A6 path: '+name)
    if args.arm=='arbiter' and args.profile_manifest not in freeze['Context_profiles']:raise ValueError('unselected Context profile')

    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):
        parser.error('fresh private output directory required; no automatic retries/resume')
    if (args.arm == 'arbiter') != bool(args.profile_manifest) or (args.profile_manifest and not valid_digest(args.profile_manifest)):
        parser.error('exact profile manifest SHA256 required for Arbiter only')
    modules = ('run_goal_condition.py', 'native_goal_client.py', 'goal_trial.py', 'goal_sources.py',
               'goal_rubric.py', 'eval_agent_smoke.py', 'qwen_export.py', 'audit_sources.py',
               'aggregate_native_retrieval.py', 'goal-evaluation-freeze.json',
               'goal-source-presentation-policy.json', 'goal-baseline-selection-freeze.json',
               'next-read-calibration-policy-freeze.json', 'goal-critical-evidence-freeze.json',
               'grade_native_retrieval.py','selected_stage_observations.py')
    paths = [Path(__file__).with_name(name) for name in modules]
    paths += [args.campaign_freeze,args.fixture_ready, args.index_bindings, args.request_template, args.question_scopes,
              args.rubric, args.fixture/'manifest.private.json']
    retained = {path: path.read_bytes() for path in paths}
    sha = lambda raw: hashlib.sha256(raw).hexdigest()
    for path,label in ((args.fixture_ready,'ready'),(args.index_bindings,'bindings'),(args.request_template,'request'),(args.question_scopes,'scopes'),(args.rubric,'rubric'),(args.fixture/'manifest.private.json','source_manifest')):
        if sha(retained[path])!=expected_pins[label]:raise ValueError('unreviewed selected A6 input: '+label)
    scopes = json.loads(retained[args.question_scopes]);rubric = json.loads(retained[args.rubric])
    split = 'test'
    tasks = {row['id']: row for row in rubric['tasks'] if row['split'] == split and row['goal_selected']}
    if len(tasks) != 100 or {row['id'] for row in scopes['questions']} != set(tasks):
        raise ValueError('frozen goal population changed')
    binding_receipt = json.loads(retained[args.index_bindings]);ready = json.loads(retained[args.fixture_ready])
    if binding_receipt.get('schema') != 'airk.private-arbiter-index-bindings/1' or binding_receipt.get('evaluation') is not True or binding_receipt.get('release_qualified') is not False or binding_receipt['scopes_sha256'] != expected_pins['scopes'] or binding_receipt['instance'] != ready['health']['instance'] or binding_receipt['namespace'] != ready['health']['catalog_clock']['namespace'] or not Path(binding_receipt['control_root']).samefile(args.control_root):
        raise ValueError('actual native index binding authority differs')
    bindings = binding_receipt['bindings']
    output=guard_output_location(output,args.fixture,bindings)
    by_root = {scope_key(row['source_root']): row['index_id'] for row in bindings}
    if len(by_root) != len(bindings) or not all(isinstance(value, str) and value for value in by_root.values()) or len(set(by_root.values())) != len(by_root) or {scope_key(row['source_root']) for row in scopes['questions']} != set(by_root):
        raise ValueError('actual native index bindings missing/duplicated')
    template = json.loads(retained[args.request_template])
    manifest = json.loads(retained[args.fixture/'manifest.private.json'])
    output.mkdir(parents=True);(output/'source-snapshots').mkdir()
    for path, raw in retained.items():(output/'source-snapshots'/sha(raw)).write_bytes(raw)
    now = lambda: datetime.now(timezone.utc).isoformat()
    before_started = now();before = audit_pass(args.fixture, manifest)
    before_finished = now()
    source_metadata=FrozenSourceMetadata(args.fixture,expected_pins['source_manifest'])
    first_question = scopes['questions'][0]
    first_client_started = time.monotonic()
    first_client = NativeGoalClient(args.runtime_pair, args.control_root,
                                   by_root[scope_key(first_question['source_root'])], output/'calls-000', args.fixture_ready,
                                   index_bindings=args.index_bindings)
    stage_before=first_client.observe_stage_status(output/'stage-status','before',args.profile_manifest if args.arm=='arbiter' else None,args.focus_profile)
    first_client_seconds = time.monotonic()-first_client_started
    load_start = time.monotonic();model, tokenizer, asset_seals = load_fixed_agent(args.assets)
    load_seconds = time.monotonic() - load_start
    result = {'schema': 'airk.private-native-goal-condition/1', 'complete': False,
              'evaluation': True, 'release_qualified': False, 'split': args.split, 'arm': args.arm,
              'expected_profile': args.profile_manifest,'expected_Focus_profile':args.focus_profile,'campaign_freeze_sha256':args.campaign_freeze_sha256,'stage_status_before':stage_before, 'started_at_utc': started_utc,
              'native_fixture_ready_sha256': sha(retained[args.fixture_ready]),
              'native_index_bindings_sha256': sha(retained[args.index_bindings]),
              'question_scopes_sha256': sha(retained[args.question_scopes]),
              'rubric_sha256': sha(retained[args.rubric]),
              'request_template_sha256': sha(retained[args.request_template]),
              'input_seals': {str(path): sha(raw) for path, raw in retained.items()},
              'agent_asset_seals': asset_seals, 'agent_load_seconds': load_seconds,
              'source_before': {'started_at_utc': before_started, 'finished_at_utc': before_finished,
                                'closed_roster': before}, 'trials': [],
              'scope': 'Actual fixed local agent and native resident canonical reads, no gold feedback, hidden retries, fake readiness, source writes or release qualification. Native test fixture service identity is distinct from client bundle identity.'}
    receipt_path = output/'condition.private.json'
    save = lambda: receipt_path.write_text(json.dumps(result, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')
    save()
    try:
        for index, question in enumerate(scopes['questions']):
            identifier = question['id'];task = tasks[identifier]
            if task['question'] != question['question']:raise ValueError('question text changed')
            client_started = time.monotonic()
            client = first_client if index == 0 else NativeGoalClient(args.runtime_pair, args.control_root,
                                      by_root[scope_key(question['source_root'])], output/f'calls-{index:03d}', args.fixture_ready,
                                      index_bindings=args.index_bindings)
            client_seconds = first_client_seconds if index == 0 else time.monotonic()-client_started
            ledger = SourceLedger(args.fixture, question['source_root'],expected_pins['source_manifest'],native_bindings=args.index_bindings,metadata_cache=source_metadata)
            generate = lambda text, records: generate_answer(model, tokenizer, text, records)
            trial = None;close_started = None
            try:
                trial = GoalTrial(client, ledger, generate, template, args.arm, args.profile_manifest,args.focus_profile,lambda: SourceLedger(args.fixture,question['source_root'],expected_pins['source_manifest'],native_bindings=args.index_bindings,metadata_cache=source_metadata)).run(task)
            finally:
                close_started = time.monotonic()
                if trial is not None:
                    result['trials'].append(trial);save()
                closed = client.close()
                if trial is not None:
                    trial.update(client_close_seconds=time.monotonic()-close_started,client_close=closed)
                    require_normal_session_close(closed)
            trial.update(**original_task_metadata(task,args.split),client_initialization_seconds=client_seconds)
            save()
            print(json.dumps({'completed_questions': index+1, 'planned_questions': len(tasks), 'arm': args.arm}), flush=True)
            if trial['failure'] in INFRA_FAILURES:raise ValueError('native infrastructure failure: '+str(trial['failure']))
        after_started = now();after = audit_pass(args.fixture, manifest)
        result['source_after'] = {'started_at_utc': after_started, 'finished_at_utc': now(), 'closed_roster': after}
        result['stage_status_after']=first_client.observe_stage_status(output/'stage-status','after',args.profile_manifest if args.arm=='arbiter' else None,args.focus_profile)
        if result['stage_status_after']['full_native_response']['configuration'] != stage_before['full_native_response']['configuration']:raise ValueError('native stage configuration changed during campaign')
        first_client.verify()
        if after != before or any(path.read_bytes() != raw for path, raw in retained.items()) or any(qwen_export.digest(path) != pin for path, pin in asset_seals.items()):
            raise ValueError('goal campaign source/asset/input drift')
        result['complete'] = True
    except Exception as error:
        result['campaign_error'] = str(error)
        raise
    finally:
        result['finished_at_utc'] = now();result['wall_seconds'] = time.monotonic()-started;save()


if __name__ == '__main__':main()
