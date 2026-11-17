# SPDX-License-Identifier: Apache-2.0
"""Strict bounded extraction from this campaign's owned stderr; never copy raw logs."""
import json
import re

METRICS = ('QueueDecode', 'QueueEncode', 'QueueSelect', 'QueueRetain', 'SemanticUpdate',
           'FullFtsCapture', 'FullFtsDocs', 'FullFtsShadow', 'FullFtsAux', 'PublicationPrepare',
           'PublicationApply', 'PublicationCommit', 'ResourceRead', 'ResourceWrite',
           'EmbeddingBatch', 'JournalMaps', 'JournalInputs', 'JournalMetadata')
FIELDS = {'calls', 'failures', 'elapsed_ns', 'items', 'bytes', 'saturated'}
HEX64 = re.compile('[0-9a-f]{64}')
KINDS = {'index','reconcile','vectors','model_setup','rebuild','inspect','plan_inspect','configure','maintain','repair','verify','recover'}
MAX_BYTES, MAX_LINE, MAX_RECORDS, MAX_JOBS = 16 << 20, 16384, 2048, 64

def integer(value):
    return type(value) is int and 0 <= value <= 2**64 - 1

def install(fixture, nonce, runtime):
    fixture.environment.pop('AIRK_PRODUCER_PROBE', None)
    fixture.environment['AIRK_PRODUCER_PROBE'] = nonce + ':' + runtime['install_sha256']
    state = {'nonce': nonce, 'runtime': runtime, 'parents': set(), 'jobs': {}, 'job_limit': False}
    rpc, start = fixture.rpc, fixture.start
    def tracked_rpc(*args, **kwargs):
        result = rpc(*args, **kwargs)
        job = result.get('job') if isinstance(result, dict) else None
        if isinstance(job, dict) and isinstance(job.get('id'), str) and HEX64.fullmatch(job['id']) and isinstance(job.get('project'), str) and HEX64.fullmatch(job['project']):
            if len(state['jobs']) < MAX_JOBS or job['id'] in state['jobs']:
                old = state['jobs'].get(job['id'])
                if old is None or old['project'] == job['project']:
                    state['jobs'][job['id']] = {'kind': job.get('kind') if job.get('kind') in KINDS else (old.get('kind') if old else None), 'project': job['project'], 'terminal_state': job.get('state') if job.get('state') in ('succeeded', 'failed', 'canceled') else (old.get('terminal_state') if old else None)}
            else:
                state['job_limit'] = True
        return result
    def tracked_start(*args, **kwargs):
        result = start(*args, **kwargs)
        if fixture.child is not None:
            state['parents'].add(fixture.child.pid)
        return result
    fixture.rpc, fixture.start = tracked_rpc, tracked_start
    return state

def collect(fixture, state):
    output = {'schema': 1, 'enabled': True, 'interval_basis': 'execute_and_completed_producer_boundaries',
              'job_terminal_basis': 'existing_owned_rpc_only', 'jobs': [], 'rejected_records': 0,
              'records_seen': 0, 'truncated': state['job_limit'], 'read_limit_bytes': MAX_BYTES}
    workers, jobs = {}, {}
    expected = {'schema', 'run', 'runtime_install_sha256', 'native_version', 'project', 'job',
                'worker', 'service', 'fencing_generation', 'boundary', 'process_id', 'sequence', 'stage', 'outcome', 'elapsed_ns', 'suppressed_samples', 'metrics'}
    def reject(): output['rejected_records'] += 1
    with (fixture.owner / 'service.stderr').open('rb') as stream:
        consumed = 0
        while consumed < MAX_BYTES and output['records_seen'] < MAX_RECORDS:
            raw = stream.readline(min(MAX_LINE + 1, MAX_BYTES - consumed))
            if not raw: break
            consumed += len(raw)
            if len(raw) > MAX_LINE or not raw.endswith(b'\n'):
                output['truncated'] = True
                break
            owner = raw.startswith(b'AIRK_PRODUCER_OWNER_V1 ')
            if not owner and not raw.startswith(b'AIRK_PRODUCER_V1 '): continue
            output['records_seen'] += 1
            try:
                value = json.loads(raw.split(b' ', 1)[1])
                if not isinstance(value, dict) or type(value.get('schema')) is not int or value.get('schema') != 1 or value.get('run') != state['nonce'] or value.get('runtime_install_sha256') != state['runtime']['install_sha256']:
                    reject(); continue
                if owner:
                    if set(value) != {'schema','run','runtime_install_sha256','parent_process_id','worker_process_id','project','worker','service'} or value['parent_process_id'] not in state['parents'] or not integer(value['worker_process_id']) or not isinstance(value['project'], str) or not HEX64.fullmatch(value['project']) or len(workers) >= 128:
                        reject(); continue
                    if any(not isinstance(value[field],str) or re.fullmatch('[0-9a-f]{32}', value[field]) is None for field in ('worker','service')):
                        reject(); continue
                    if any(worker == value['worker'] and (pid != value['worker_process_id'] or binding != (value['project'],value['service'])) for (pid,worker),binding in workers.items()):
                        reject(); continue
                    workers[(value['worker_process_id'],value['worker'])] = (value['project'],value['service'])
                    continue
                if set(value) != expected or value['native_version'] != state['runtime']['version'] or value['job'] not in state['jobs'] or workers.get((value['process_id'],value['worker'])) != (value['project'],value['service']) or state['jobs'][value['job']]['project'] != value['project']:
                    reject(); continue
                if not all(integer(value[key]) for key in ('sequence','elapsed_ns','suppressed_samples','fencing_generation')) or not integer(value['process_id']) or value['process_id'] == 0 or value['fencing_generation'] == 0 or value['sequence'] > 130 or value['stage'] not in ('started','producer_boundary','execute_finished') or value['outcome'] not in ('in_progress','failed','completed_interval','execute_succeeded','execute_canceled','execute_non_success','execute_error'):
                    reject(); continue
                allowed = {'started': {'in_progress'}, 'producer_boundary': {'failed','completed_interval'},
                           'execute_finished': {'execute_succeeded','execute_canceled','execute_non_success','execute_error'}}
                if value['outcome'] not in allowed[value['stage']] or (value['boundary'] not in ('SemanticUpdate','PublicationCommit','FullFtsCapture','EmbeddingBatch') if value['stage'] == 'producer_boundary' else value['boundary'] is not None):
                    reject(); continue
                metrics = value['metrics']
                if not isinstance(metrics, dict) or (set(metrics) != set(METRICS) if value['stage'] != 'started' else bool(metrics)):
                    reject(); continue
                if any(not isinstance(work,dict) or set(work) != FIELDS or type(work['saturated']) is not bool or not all(integer(work[field]) for field in FIELDS - {'saturated'}) or work['failures'] > work['calls'] for work in metrics.values()):
                    reject(); continue
                key = (value['job'], value['worker'])
                if key not in jobs and len(jobs) >= MAX_JOBS:
                    output['truncated'] = True
                    continue
                if key not in jobs:
                    if value['stage'] != 'started' or value['sequence'] != 0 or value['elapsed_ns'] != 0 or value['suppressed_samples'] != 0:
                        reject(); continue
                    jobs[key] = {'job': value['job'], 'project': value['project'], 'process_id':value['process_id'], 'worker':value['worker'], 'service':value['service'], 'fencing_generation':value['fencing_generation'], 'first':value, 'last':value, 'first_failed_interval':None, 'observed_intervals':0, 'sequence_gap':False, 'counter_regression':False, 'invalid_framing':False}
                row = jobs[key]
                if row['observed_intervals']:
                    stable = all(value[field] == row[field] for field in ('project','process_id','worker','service','fencing_generation'))
                    if not stable or row['last']['stage'] == 'execute_finished' or value['stage'] == 'started' or value['sequence'] != row['last']['sequence'] + 1:
                        row['invalid_framing'] = True
                        row['sequence_gap'] |= value['sequence'] != row['last']['sequence'] + 1
                        reject(); continue
                previous = row['last']
                if value['elapsed_ns'] < previous['elapsed_ns'] or value['suppressed_samples'] < previous['suppressed_samples'] or any(
                        name in previous['metrics'] and any(value['metrics'][name][field] < previous['metrics'][name][field]
                            for field in FIELDS - {'saturated'}) for name in value['metrics']):
                    row['counter_regression'] = True
                row['last'] = value
                row['observed_intervals'] += 1
                if value['outcome'] in ('failed','execute_error','execute_non_success','execute_canceled') and row['first_failed_interval'] is None:
                    row['first_failed_interval'] = value
            except (ValueError, TypeError, KeyError):
                reject()
        if consumed >= MAX_BYTES or output['records_seen'] >= MAX_RECORDS:
            output['truncated'] = True
    for row in jobs.values():
        row['partial'] = row['first']['stage'] != 'started' or row['last']['stage'] != 'execute_finished' or row['sequence_gap'] or row['counter_regression'] or row['invalid_framing']
        row['job_kind'] = state['jobs'][row['job']].get('kind')
        row['corpus_alias'] = next((alias for alias, project in fixture.projects.items() if alias in ('airk','bookbuilder') and project.get('binding') == row['project']), None)
        row['intermediate_intervals_suppressed'] = row['last']['suppressed_samples']
        row['job_terminal_observed'] = state['jobs'][row['job']]['terminal_state']
        output['jobs'].append(row)
    output['missing_job_intervals'] = sum(not any(key[0] == job for key in jobs) for job in state['jobs'])
    output['coverage'] = 'partial' if output['truncated'] or output['rejected_records'] or output['missing_job_intervals'] or any(row['partial'] for row in output['jobs']) else ('observed' if jobs else 'not_observed')
    return output
