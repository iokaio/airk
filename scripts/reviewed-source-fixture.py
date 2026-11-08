# SPDX-License-Identifier: Apache-2.0
"""Bounded canonical source reviews for owned modern native qualification.

Native handlers validate/authenticate the complete fence proof. This adapter
binds their full response to the exact submitted review; disk evidence grants
no authority, and no mutation is replayed on unknown outcomes.
"""
import json
from pathlib import Path
import re
import time
import uuid


def hex_identity(value, length=64):
    assert isinstance(value, str) and re.fullmatch('[0-9a-f]{%d}' % length, value), 'Invalid native identity'


def validate_fenced(receipt, draft, registration):
    request, preview = draft['request'], draft['preview']
    assert receipt['request'] == request and receipt['preview_digest'] == draft['preview_digest']
    metadata = receipt['metadata']
    assert metadata['schema'] == 1 and metadata['caller'] == request['caller'] and metadata['key'] == request['key']
    assert metadata['request_digest'] == preview['request_digest']
    assert metadata['catalog_revision'] == request['expected_catalog_revision'] + 1
    assert metadata['authority_fence_required'] is True
    hex_identity(metadata['operation_id'], 32); hex_identity(receipt['prepared_digest'])
    proof = receipt['proof']
    hex_identity(proof['plan_digest']); hex_identity(proof['service'], 32)
    assert type(proof['gate_generation']) is int and proof['gate_generation'] > 0
    assert isinstance(proof['confirmations'], list) and len(proof['confirmations']) <= 128
    for confirmation in proof['confirmations']:
        assert confirmation['method'] in ('native_scope_ack', 'native_worker_reap')
        assert isinstance(confirmation['target']['worker'], dict) and isinstance(confirmation['target']['scope'], dict)
    if request['operation']['operation'] == 'retire_index':
        obligation = proof['worker_obligation']
        assert obligation['index_id'] == request['operation']['id'] and obligation['backing'] == registration['binding']
        workers = obligation['workers']
        confirmations = proof.get('worker_confirmations', [])
        canonical = lambda value: json.dumps(value, sort_keys=True, separators=(',', ':'))
        assert len(workers) <= 128 and len(workers) == len(confirmations)
        assert len({canonical(worker) for worker in workers}) == len(workers)
        assert sorted(canonical(worker) for worker in workers) == sorted(canonical(item['worker']) for item in confirmations)
        assert all(item['method'] == 'native_worker_reap' for item in confirmations)
        assert all(worker['project'] == registration['binding'] and worker['service'] == proof['service'] for worker in workers)
        assert all(item['worker'] in workers for item in obligation.get('providers', []))
    else:
        assert proof.get('worker_obligation') is None and not proof.get('worker_confirmations')
    return metadata


class SourceReview:
    def __init__(self, fixture, name):
        assert fixture.single_image
        self.fixture, self.name = fixture, name
        self.registration = dict(fixture.projects[name])
        self.selected = dict(fixture.scopes[name])
        self.deadline = time.monotonic() + min(fixture.job_timeout, 180)
        self.calls, self.bytes, self.records = 0, 0, 0
        self.evidence = fixture.owner / ('source-review-' + uuid.uuid4().hex)
        self.evidence.mkdir(mode=0o700)

    def retain(self, value):
        raw = json.dumps(value, ensure_ascii=False).encode('utf-8')
        self.bytes += len(raw)
        assert self.bytes <= 8*1024*1024, 'Native source review evidence budget exhausted'
        self.records += 1
        with (self.evidence / ('%04d.json' % self.records)).open('xb') as stream:
            stream.write(raw)

    def rpc(self, method):
        assert time.monotonic() < self.deadline and self.calls < 1024, 'Native source review observation budget exhausted'
        self.calls += 1
        response = self.fixture.rpc(method, deadline=self.deadline)
        self.retain({'method': method, 'response': response})
        return response

    def pause(self):
        assert time.monotonic()+0.1 < self.deadline, 'Native source review deadline exhausted'
        time.sleep(0.1)

    def idle(self):
        # Observe accepted work/watermarks before review. Native draft admission
        # remains authoritative if new work arrives after these observations.
        project = self.registration['binding']
        while True:
            progress = self.rpc({'progress': {'project': project}})
            watch = self.rpc({'watch': {'project': project}})
            assert progress['status'] == 'progress' and progress['project'] == project
            assert watch['status'] == 'watch' and watch['project'] == project
            state = watch['observation']
            assert state['failure'] is None, state
            pending, queued = progress['pending_job'], progress['queued_jobs']
            assert pending is None or pending['project'] == project
            assert all(job['project'] == project for job in queued)
            observed = progress.get('observation')
            assert observed is None or observed['job']['project'] == project and observed['worker']['project'] == project
            if pending is None and not queued and progress['queue_truncated'] is False and progress['project_paused'] is False and state['completed_revision'] == state['requested_revision'] and (observed is None or observed['job']['state'] not in ('pending','running')):
                return
            self.pause()

    def source_draft(self, selection):
        method = {'catalog_source_mutation_draft': {'selection': selection}}
        while True:
            response = self.rpc(method)
            if response['status'] != 'source_change_busy':
                assert response['status'] == 'catalog_source_mutation_draft', response
                return response['draft']
            # A draft has no accepted mutation. Only its exact, typed work guard
            # authorizes another observation of this same key and selection.
            busy = response['busy']
            assert response['code'] == 'source_change_jobs_pending'
            assert busy['index_id'] == selection['index_id']
            counts = (busy['pending_jobs'], busy['running_jobs'])
            assert all(type(value) is int and 0 <= value <= 4294967295 for value in counts)
            assert sum(counts) > 0
            self.pause()
            self.idle()

    def prepare(self, change):
        health = self.rpc('health')['observation']
        expected = self.fixture.managed_health
        assert all(health[field] == expected[field] for field in ('instance','runtime_sha256','executable_sha256'))
        assert health['catalog_clock']['namespace'] == self.selected['namespace'] and health['policy']['accepting'] is True
        self.idle()
        descriptor = self.rpc({'catalog_host_context': {'key': self.selected['context_key'], 'expected_catalog_revision': None}})['descriptor']
        assert descriptor['namespace'] == self.selected['namespace']
        epoch = self.rpc('request_epoch')
        assert epoch['namespace'] == self.selected['namespace']
        key = {'namespace': epoch['namespace'], 'epoch': epoch['epoch'], 'nonce': uuid.uuid4().hex}
        draft = self.source_draft({'key': key, 'expected_catalog_revision': descriptor['catalog_revision'],
            'index_id': self.selected['index_id'], 'change': change})
        request, preview = draft['request'], draft['preview']
        assert request['key'] == key and request['expected_catalog_revision'] == descriptor['catalog_revision']
        assert request['operation']['id'] == self.selected['index_id']
        assert preview['operation'] == request['operation'] and preview['catalog_revision'] == request['expected_catalog_revision']
        assert preview['next_revision'] == request['expected_catalog_revision']+1 and preview['authority_fence_required'] is True
        hex_identity(preview['request_digest']); hex_identity(draft['preview_digest'])
        operation = request['operation']
        if change['operation'] == 'retire_index':
            assert operation['operation'] == 'retire_index' and operation['reason'] == {'reason': 'explicit'}
        else:
            assert operation['operation'] == 'begin_index_source_change'
            plan = operation['plan']; assert plan['registration'] == self.registration
            configuration = plan['configuration']['configuration']
            assert configuration['operation'] == change['operation'] and configuration['alias'] == change['alias']
            if change['operation'] == 'add_root':
                assert Path(configuration['path']).samefile(change['path']), 'Native source review changed the selected physical root'
            assert Path(plan['before']['primary_root']).samefile(self.registration['root'])
            assert plan['before']['primary_root'] == plan['after']['primary_root']
        return draft

    def apply(self, draft):
        request = draft['request']
        response = None
        try:
            response = self.rpc({'catalog_apply': {'request': request, 'preview_digest': draft['preview_digest']}})
        except Exception as error:
            self.retain({'unknown_apply': str(error)})
            # Observe the original key; no newly generated request or Apply.
        if response is not None:
            if response['status'] == 'catalog_outcome_unknown':
                assert response['caller'] == request['caller'] and response['key'] == request['key']
                assert response['request_digest'] == draft['preview']['request_digest']
            else:
                assert response['status'] == 'catalog_applied', response
                if draft['preview']['authority_fence_required']:
                    validate_fenced(response['fenced_receipt'], draft, self.registration)
        while True:
            observed = self.rpc({'catalog_receipt': {'caller': request['caller'], 'key': request['key']}})
            assert observed['status'] == 'catalog_receipt' and observed.get('expired') is not True
            fence = observed.get('fence')
            metadata = observed.get('receipt')
            if not draft['preview']['authority_fence_required'] and metadata is not None:
                assert metadata['schema'] == 1 and metadata['caller'] == request['caller'] and metadata['key'] == request['key']
                assert metadata['request_digest'] == draft['preview']['request_digest'] and metadata['authority_fence_required'] is False
                assert metadata['catalog_revision'] == request['expected_catalog_revision']+1 and fence is None
                hex_identity(metadata['operation_id'],32)
                return {'request':request,'metadata':metadata,'preview_digest':draft['preview_digest']}
            if fence is not None and fence['state'] == 'applied':
                full = fence['value']; metadata = validate_fenced(full, draft, self.registration)
                assert observed['receipt'] == metadata
                return full
            if fence is not None and fence['state'] == 'pending':
                assert fence['value']['request'] == request and fence['value']['preview_digest'] == draft['preview_digest']
            else:
                assert fence is None, observed
            self.pause()

    def grant_sources(self):
        # Root mutations do not widen an old grant. Explicitly review a new
        # Project/membership and native grant over the current source roster.
        descriptor = self.rpc({'catalog_host_context': {'key': self.selected['context_key'], 'expected_catalog_revision': None}})['descriptor']
        assert descriptor['namespace'] == self.selected['namespace']
        revision = descriptor['catalog_revision']
        response = self.rpc({'catalog_page': {'request': {'kind':'sources','index_id':self.selected['index_id'],
            'project_id':None,'after':None,'expected_catalog_revision':revision,'limit':32,'max_bytes':48000}}})
        page = response['page']
        assert page['namespace'] == self.selected['namespace'] and page['revision'] == revision and page['continuation'] is None
        sources = []
        for item in page['items']:
            assert item['kind'] == 'source' and item['value']['index_id'] == self.selected['index_id']
            if item['value']['enabled']:
                sources.append(item['value']['id'])
        assert sources and len(set(sources)) == len(sources) and len(sources) <= 32
        project = 'prj_'+uuid.uuid4().hex
        membership = {'project_id':project,'index_id':self.selected['index_id'],'revision':1,
                      'scope':[{'source_root_id':source,'kind':'subtree','path':''} for source in sorted(sources)]}
        for operation in [{'operation':'create_project','id':project,'name':'Explicit storage fixture root scope'},
                          {'operation':'set_membership','membership':membership,'expected_scope_revision':1,'expected_membership_revision':None}]:
            epoch = self.rpc('request_epoch'); assert epoch['namespace'] == self.selected['namespace']
            request = {'caller':uuid.uuid4().hex,'key':{'namespace':epoch['namespace'],'epoch':epoch['epoch'],'nonce':uuid.uuid4().hex},
                       'expected_catalog_revision':revision,'operation':operation}
            reviewed = self.rpc({'catalog_preview':{'request':request}})
            assert reviewed['status'] == 'catalog_preview' and reviewed['preview']['operation'] == operation
            assert reviewed['preview']['catalog_revision'] == revision and reviewed['preview']['next_revision'] == revision+1
            result = self.apply({'request':request,'preview':reviewed['preview'],'preview_digest':reviewed['preview_digest']})
            revision = result['metadata']['catalog_revision']
        epoch = self.rpc('request_epoch'); assert epoch['namespace'] == self.selected['namespace']
        response = self.rpc({'catalog_grant_draft': {'context_key':self.selected['context_key'],'selection':{
            'key':{'namespace':epoch['namespace'],'epoch':epoch['epoch'],'nonce':uuid.uuid4().hex},'expected_catalog_revision':revision,
            'project_ids':[project],'index_ids':[self.selected['index_id']]}}})
        assert response['status'] == 'catalog_grant_draft'
        draft = response['draft']; grant = draft['request']['operation']['grant']
        assert draft['request']['operation']['operation'] == 'create_grant' and grant['revision'] == 1 and grant['revoked'] is False
        assert len(grant['memberships']) == 1 and grant['memberships'][0]['membership'] == membership
        result = self.apply(draft)
        selected = {**self.selected,'expected_catalog_revision':result['metadata']['catalog_revision'],
                    'grant_id':grant['id'],'expected_grant_revision':1,'project_ids':[project]}
        assert selected['grant_id'] != self.selected['grant_id']
        self.retain({'original_selection':self.selected,'explicit_replacement_selection':selected})
        self.fixture.scopes[self.name] = selected

    def source_change(self, change):
        draft = self.prepare(change); begin = self.apply(draft)
        while True:
            response = self.rpc({'catalog_source_change_observation': {'begin_operation': begin['metadata']['operation_id']}})
            assert response['status'] == 'catalog_source_change_observation'
            observation = response['observation']; assert observation['begin'] == begin
            job = observation['job']
            if job is not None:
                assert job['id'] == draft['request']['operation']['plan']['job'] and job['project'] == self.registration['binding'] and job['kind'] == 'configure'
                assert job['state'] not in ('failed', 'canceled'), observation
            completion = observation['completion']
            if completion is not None:
                assert job is not None and job['state'] == 'succeeded'
                assert completion['caller'] == draft['request']['caller'] and completion['key']['namespace'] == self.selected['namespace']
                assert completion['authority_fence_required'] is False and completion['catalog_revision'] > begin['metadata']['catalog_revision']
                self.grant_sources()
                return observation
            self.pause()

    def retire(self):
        draft = self.prepare({'operation': 'retire_index'}); applied = self.apply(draft)
        response = self.rpc({'catalog_index_retirement_evidence': {'index_id': self.selected['index_id']}})
        assert response['status'] == 'catalog_index_retirement_evidence' and response['index_id'] == self.selected['index_id']
        evidence = response['evidence']; assert evidence['retirement'] == applied
        index = evidence['index']; operation = draft['request']['operation']
        assert index['id'] == self.selected['index_id'] and index['backing'] == self.registration and index['tombstone'] is True
        assert index['definition_revision'] == operation['expected_definition_revision']+1 and index['source_revision'] == operation['expected_source_revision']+1
        assert evidence['catalog_clock']['namespace'] == self.selected['namespace'] and evidence['catalog_clock']['revision'] >= applied['metadata']['catalog_revision']
        return response
