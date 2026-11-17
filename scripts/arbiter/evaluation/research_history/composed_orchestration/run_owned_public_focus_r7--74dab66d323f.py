# SPDX-License-Identifier: Apache-2.0
"""Opt-in private multi-descendant Job launcher; no native request construction."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time
SID='S-1-5-21-3678370870-3858642417-826968105-1002'

def require(value,message):
    if not value:raise ValueError(message)

def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def load(path):
    spec=importlib.util.spec_from_file_location('reviewed_multi_descendant_owner',path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def natural_owner(value):
    return (isinstance(value,dict) and value.get('exit_code')==0
        and value.get('timed_out') is False and value.get('log_oversized') is False
        and value.get('owned_descendants_settled') is True
        and value.get('owned_job_forced_termination') is False
        and value.get('owned_job_final_active_processes')==0
        and value.get('owned_job_final_member_pids')==[]
        and value.get('child_assigned_before_resume') is True
        and value.get('parent_assigned_to_job') is False
        and 'cleanup_refusal' not in value)

def one_step(output,label):
    paths=list(output.glob('*-'+label+'.json'))
    require(len(paths)==1,'exact one original observation required: '+label)
    return json.loads(paths[0].read_text(encoding='utf-8'))

def validate_native_observations(report,observations,pins):
    require(report.get('passed') is True and report.get('native_nn_calls_claimed') is True
        and report.get('release_qualified') is False
        and report.get('large_public_finalize_exercised') is False,
        'child acceptance absent; acquisition/release claims cannot substitute')
    first=observations['actual-start']; restarted=observations['actual-restart-health']
    require(first['user']==restarted['user']==SID
        and first['executable_sha256']==restarted['executable_sha256']==pins['airk_sha256']
        and first['runtime_sha256']==restarted['runtime_sha256']
        and first['version']==restarted['version']=='1.8.207'
        and first['instance']!=restarted['instance'],
        'actual same-image new-instance native lease reacquisition absent')
    require(first.get('build',{}).get('source_sha256')==restarted.get('build',{}).get('source_sha256')==pins['source_sha256'],
        'actual native source identity differs')
    for label,health in [('first',first),('final',restarted)]:
        stop=observations[label+'-native-stop']; record=stop['record']; request=record['request']
        require(stop.get('status')=='lifecycle' and record.get('phase')=='complete'
            and request['action']=='stop' and request['expected_instance']==health['instance']
            and request['runtime']==health['runtime_sha256'] and request['manager'] is False,
            'actual original native StopComplete missing: '+label)
        exited=observations[label+'-process-exit']
        require(exited.get('process_handle_signaled') is True and exited.get('exit_code')==0
            and exited.get('pid')==health['process_id'],
                'actual original service process exit unobserved: '+label)
    require(report.get('actual_native_stop')==observations['final-native-stop']
        and report.get('source_files_unchanged') is True and report.get('import_source_files_unchanged') is True,
        'final native stop/source preservation differs')
    require(all(type(child.get('exit_code')) is int and child['exit_code']==0
                for child in report.get('actual_controller_children',[]))
        and report.get('actual_controller_children'), 'owned native controller remains unknown')
    require('cleanup_failure' not in report and 'failure' not in report,'child cleanup failure retained')
    return {'native_stop_and_original_process_exits':True,
            'actual_new_instance_same_image_lease_reacquisition':True,
            'scope':'isolated U93 managed public control; modern I4 installed journey unqualified'}

def validate_card(card):
    require(card['schema']=='airk.private-public-focus-outer-card/1','unreviewed outer card')
    require(card['whole_phase_seconds']==1800 and card['native_read_ceiling_seconds']==120,
            'original reviewed phase/native ceiling changed')
    require(card['owned_entrypoint']=='run_owned','wrong maintained multi-process owner entrypoint')
    require(0<len(card['pins'])<=4096,'source pin roster out of bounds')
    names=[os.path.normcase(str(Path(p['path']).resolve(strict=True))) for p in card['pins']]
    require(len(set(names))==len(names),'duplicate source pins')
    for pin in card['pins']:
        require(digest(pin['path'])==pin['sha256'],'consumed source/artifact input changed: '+pin['path'])
    require(card['argv'][0]==card['python']['path'] and card['argv'][1]=='-B','exact reviewed base Python required')
    require('--whole-phase-seconds' in card['argv']
        and card['argv'][card['argv'].index('--whole-phase-seconds')+1]=='1800',
        'child original phase differs')
    return card

def run(card_path,expected):
    require(os.name=='nt','actual outer process owner qualifies Windows only')
    actual=subprocess.check_output([r'C:\Windows\System32\whoami.exe','/user','/fo','csv','/nh'],text=True,timeout=10)
    require(SID in actual,'effective account differs from active verified human')
    require(digest(card_path)==expected,'reviewed execution card differs')
    card=validate_card(json.loads(Path(card_path).read_text(encoding='utf-8')))
    output=Path(card['outer_output']); journey=Path(card['child_output']); fixture=Path(card['fixture'])
    repository=Path(card['repository']).resolve(strict=True)
    require(output.resolve().is_relative_to(repository/'artifacts')
        and journey.resolve().is_relative_to(repository/'artifacts'),'private receipt output leaves artifacts')
    require(output.parent.is_dir() and journey.parent.is_dir() and fixture.parent.is_dir(),
        'Root must prepare and verify protected output/fixture parents first')
    require(not output.exists() and not journey.exists() and not fixture.exists(),'fresh receipt/fixture targets required')
    output.mkdir()
    receipt={'schema':'airk.private-public-focus-owned-execution/1','passed':False,
        'release_qualified':False,'modern_i4_installed_journey_qualified':False,
        'execution_card':{'path':str(card_path),'sha256':expected},'effective_user_sid':SID,
        'whole_phase_seconds':card['whole_phase_seconds'],'native_read_ceiling_seconds':120,
        'cleanup_observation_seconds':5,'argv':card['argv'],'environment_override':card['environment'],
        'owner':None,'actual_native_observations':None}
    began=time.monotonic()
    try:
        owner=load(card['owner_source'])
        result=owner.run_owned(card['argv'],card['repository'],output/'owned-child.log',
                               card['whole_phase_seconds'],{**os.environ,**card['environment']})
        receipt['owner']=result
        child_path=journey/'receipt.json'
        child=json.loads(child_path.read_text(encoding='utf-8')) if child_path.is_file() else None
        receipt['child_receipt_observation']=child
        if result['exit_code']!=0:
            raise RuntimeError('journey child failed with exit '+str(result['exit_code'])
                +'; descendants_settled='+str(result['owned_descendants_settled'])
                +'; original child failure='+str(child.get('failure') if child else 'receipt unavailable'))
        require(natural_owner(result),'owned descendants did not naturally settle; preserve failed run')
        require(child is not None,'successful child receipt unavailable')
        labels=('actual-start','actual-restart-health','first-native-stop','first-process-exit',
                'final-native-stop','final-process-exit')
        observations={label:one_step(journey,label) for label in labels}
        receipt['actual_native_observations']=observations
        receipt['native_acceptance']=validate_native_observations(child,observations,card['native_identity'])
        receipt['child_receipt']={'path':str(journey/'receipt.json'),'sha256':digest(journey/'receipt.json')}
        receipt['passed']=True
    except BaseException as error:
        receipt['failure']={'type':type(error).__name__,'message':str(error),
            'owned_cleanup':getattr(error,'owned_cleanup',None)}
        raise
    finally:
        receipt['elapsed_seconds']=time.monotonic()-began
        receipt['fixture_preserved']=str(fixture)
        with (output/'receipt.json').open('x',encoding='utf-8') as stream:
            json.dump(receipt,stream,indent=2);stream.write('\n')

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--card',type=Path,required=True)
    parser.add_argument('--card-sha256',required=True)
    parser.add_argument('--consent-actual-isolated-public-focus',action='store_true',required=True)
    args=parser.parse_args();run(args.card,args.card_sha256)