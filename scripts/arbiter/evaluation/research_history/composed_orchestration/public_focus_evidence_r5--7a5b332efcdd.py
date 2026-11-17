# SPDX-License-Identifier: Apache-2.0
"""Private assertions over canonical native evidence; never a wire producer.

Combined member expansion and qualified origins belong to combined_evidence.py.
Raw envelopes and combined addresses remain untouched in retained observations.
"""
from copy import deepcopy
import json
import math


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical_body(value):
    """Only the native MCP envelope boundary, once; never recursively flatten."""
    require(isinstance(value, dict), 'native result is not an object')
    require(not value.get('isError') and not value.get('error'), 'native result refused')
    if 'content' not in value and 'structuredContent' not in value:
        return deepcopy(value)
    content = value.get('content')
    require(isinstance(content, list) and len(content) == 1
            and content[0].get('type') == 'text', 'expected one native JSON text block')
    body = json.loads(content[0]['text'])
    require(isinstance(body, dict), 'native canonical body is not an object')
    if 'structuredContent' in value:
        require(value['structuredContent'] == body, 'native text/structuredContent differ')
    require(not body.get('error') and not body.get('isError'), 'native body refused')
    return body


def observe_combined(envelope, index, oracle, evidence, quality, *, kind, expected_publications=None):
    require(kind in ('search', 'diagnostics'), 'unsupported observation purpose')
    combined = canonical_body(envelope)
    require(combined.get('schema') == 'airk.combined-read/1', 'combined read required')
    selection = combined['selection']
    require(selection['index_ids'] == [index] and selection['project_ids'] == []
            and selection.get('grant_id') is None and selection.get('grant_revision') is None
            and selection['selection']['owner'] == 'agent', 'immutable open Index selection differs')
    vector = combined['publication_vector']
    require(vector['authority'] == selection['identity']
            and vector['observed_from_unix_ms'] <= vector['observed_to_unix_ms'],
            'publication authority/observation interval differs')
    rows = vector['members']
    require(len(rows) == 1 and rows[0]['index_id'] == index and rows[0]['publication'],
            'exact selected publication vector required')
    if expected_publications is not None:
        require(rows == expected_publications, 'immutable publication changed')
    publication = evidence.publication(combined, {'member': 0}, index)
    require(not combined.get('error') and combined.get('partial') is False
            and combined.get('truncated') is False, 'complete combined observation required')
    members = combined['members']
    require(len(members) == 1 and members[0]['index_id'] == index
            and members[0]['allocation']['index_id'] == index
            and members[0]['outcome'] == 'complete' and members[0]['is_error'] is False,
            'exact complete selected member required')
    member_result = evidence.member_result(combined, index)
    leaf = canonical_body(member_result)
    require(not leaf.get('truncated'), 'complete member observation required')
    actual_publication = leaf.get('publication')
    # Diagnostics Status projects the token alone, with collection adjacent.
    if kind == 'diagnostics' and isinstance(actual_publication, dict) and set(actual_publication) == {'token'}:
        require(actual_publication['token'] == publication['token']
                and leaf['collection'] == publication['collection'], 'diagnostics publication differs')
    else:
        require(actual_publication == publication, 'canonical member publication differs')
    require(leaf['collection'] == publication['collection'], 'member collection differs')
    scope = leaf['scope_observation']
    require(scope['schema'] == 1 and scope['qualifier'] == 'selected_subset'
            and scope.get('grant') is None and scope.get('provenance') is None
            and scope['selection'] == selection['selection'], 'member source authority differs')
    scope_members = scope['members']
    roots = sorted({root for selected, root, _ in oracle['sources'] if selected == index})
    require(roots and len(scope_members) == 1 and scope_members[0]['index_id'] == index
            and scope_members[0]['memberships'] == []
            and sorted(scope_members[0]['source_root_ids']) == roots,
            'selected native source roots/Projects differ')
    ranking = combined['ranking']
    origins = []
    if kind == 'diagnostics':
        require(ranking == [], 'diagnostics unexpectedly ranks evidence')
    else:
        hits = leaf['hits']
        require(isinstance(hits, list) and hits and len(ranking) == len(hits),
                'complete search hit/ranking coverage required')
        seen = set()
        for rank in ranking:
            local = rank['local_rank']
            require(type(local) is int and 0 <= local < len(hits) and local not in seen,
                    'native local rank invalid/duplicated')
            seen.add(local)
            prefix = '/members/0/result' + ('/structuredContent' if 'structuredContent' in members[0]['result'] else '')
            require(rank['index_id'] == index and rank['result_pointer'] == prefix + '/hits/' + str(local),
                    'qualified hit locator differs')
            origin = evidence.ranked_origin(combined, rank)
            key = (origin['index_id'], origin['source_root_id'], origin['source_relative_path'])
            require(key in oracle['sources'] and origin['publication'] == publication
                    and origin['project_ids'] == oracle['sources'][key]['project_ids'] == [],
                    'qualified immutable origin differs')
            hit = quality.ranked_hit(combined, rank)
            require(hit == hits[local] and hit['citation']['content_hash'] == oracle['sources'][key]['sha256'],
                    'ranked source bytes differ')
            origins.append(origin)
    return {'envelope': deepcopy(envelope), 'combined': combined,
            'materialized_member_result': member_result, 'member_body': leaf,
            'qualified_origins': origins, 'publication_identity': deepcopy(rows), 'native_shape': 'combined'}


def require_scope(leaf,index,oracle,quality,expected_scope=None):
    scope=leaf['scope_observation']
    require(scope['schema']==1 and scope['qualifier']=='selected_subset'
        and scope.get('grant') is None and scope.get('provenance') is None
        and scope['selection']['owner']=='agent', 'native open scope authority differs')
    require(set(scope)<= {'schema','qualifier','fingerprint','selection','members','grant','provenance'}
        and set(scope['selection'])=={'owner','scope_identity','catalog_revision'}, 'unknown native scope authority')
    roots=sorted({root for selected,root,_ in oracle['sources'] if selected==index})
    members=scope['members']
    require(roots and len(members)==1 and members[0]['index_id']==index
        and members[0]['memberships']==[] and sorted(members[0]['source_root_ids'])==roots,
        'selected native source roots/Projects differ')
    require(members[0]['source_revision']>0 and members[0]['policy_epoch']>=0
        and (members[0]['policy_epoch']==0)==(members[0].get('policy_digest') is None), 'native source policy differs')
    if expected_scope is not None:
        require(scope==expected_scope,'immutable native source scope changed')
    return scope


def observe_singleton(envelope,index,oracle,evidence,quality,*,kind,request_arguments,expected_publications=None,expected_scope=None):
    # Current native Open routes one implicit index through open_single_backend.
    # A genuine singleton is retained as received; never fabricate a CombinedRead.
    require(isinstance(request_arguments,dict)
        and not {'project_ids','index_ids','member_availability'} & request_arguments.keys(),
        'singleton requires original implicit one-index request')
    permitted=({'response_mode'} if kind=='diagnostics' else
        {'query','mode','limit','max_bytes','response_mode','full_chunks','include_status'})
    require(set(request_arguments)<=permitted and request_arguments.get('response_mode')=='full',
        'singleton request recipe differs')
    leaf=canonical_body(envelope)
    require('schema' not in leaf and not {'members','ranking','selection','publication_vector','shared_values'} & leaf.keys(),
        'unknown native singleton shape')
    require(leaf['read_scope']=={'source':'named','index_ids':[index]},'exact named singleton disclosure differs')
    require_scope(leaf,index,oracle,quality,expected_scope)
    pub=leaf['publication']
    require(isinstance(pub,dict) and set(pub)=={'collection','incarnation','generation','revision','token'}
        and type(pub['generation']) is int and pub['generation']>0
        and leaf['collection']==pub['collection'] and leaf['generation']==pub['generation'],
        'native singleton publication/collection/generation differs')
    require(pub['token']=='airk-r1:'+pub['incarnation']+':'+str(pub['generation'])+':'+pub['revision'],
        'native singleton publication token differs')
    if expected_publications is not None:
        require(pub==expected_publications,'immutable publication changed')
    if kind=='diagnostics':
        require('hits' not in leaf and 'semantic_readiness' in leaf and 'discovery_readiness' in leaf,
            'native singleton diagnostics required')
    else:
        require(type(leaf['truncated']) is bool and isinstance(leaf['hits'],list) and len(leaf['hits'])<=100,
            'bounded native singleton SearchResponse required')
        # This checks source identities, never upgrades a partial/unchecked
        # verification status or claims source-check semantic qualification.
        verification=leaf['source_verification']
        require(verification['schema']=='airk.saved-source-verification/1'
            and len(evidence.canonical(verification))<=32768,'bounded native saved-source receipt required')
        origins=verification['origins'];sources=verification['sources'];references=verification['references']
        require(len(origins)<=32 and len(sources)<=4096 and len(references)<=4096,'native verification roster exceeds bounds')
        origin_ids=[r['id'] for r in origins];source_ids=[r['id'] for r in sources];reference_ids=[r['id'] for r in references]
        require(len(set(origin_ids))==len(origin_ids) and len(set(source_ids))==len(source_ids)
            and len(set(reference_ids))==len(reference_ids),'duplicate native verification identities')
        for origin in origins:
            require(origin['index_id']==index and origin['project_ids']==[]
                and origin['collection']==pub['collection'] and origin['incarnation']==pub['incarnation']
                and origin['publication']==pub['token'],'singleton verification origin differs')
        def citation_key(citation):
            keys=[key for key,row in oracle['sources'].items() if key[0]==index
                and row['virtual_path']==citation['path'] and row['sha256']==citation['content_hash']]
            require(len(keys)==1,'singleton citation source/hash differs')
            return keys[0]
        for hit in leaf['hits']:citation_key(hit['citation'])
        catalog_roots={item['value']['id']:item['value']['canonical_path']
            for item in oracle['native_catalog']['page']['items'] if item['kind']=='source'}
        for source in sources:
            key=(index,source['source_root_id'],source['source_relative_path'])
            require(key in oracle['sources'] and source['origin_id'] in origin_ids
                and source['path']==oracle['sources'][key]['virtual_path'] and source['project_ids']==[]
                and quality.location_key(source['original_root'])==quality.location_key(catalog_roots[key[1]])
                and all(source.get(field) is None or source[field]==oracle['sources'][key]['sha256']
                    for field in ('expected_hash','observed_hash')), 'singleton verification source/root/hash differs')
        for ref in references:
            # Native references remain relative to this native body, exactly.
            pointer=ref['result_pointer']
            require(ref['kind']=='citation' and pointer.startswith('/hits/') and pointer.endswith('/citation'),
                'native singleton citation pointer required')
            ordinal=pointer[len('/hits/'):-len('/citation')]
            require(ordinal.isascii() and ordinal.isdecimal() and str(int(ordinal))==ordinal
                and int(ordinal)<len(leaf['hits']),'native singleton citation pointer invalid')
            citation=leaf['hits'][int(ordinal)]['citation'];key=citation_key(citation)
            matches=[source for source in sources if source['id']==ref['source_id']]
            require(len(matches)==1 and matches[0]['source_root_id']==key[1]
                and matches[0]['source_relative_path']==key[2]
                and ref['id'] in matches[0]['reference_ids'],'singleton reference rebound to another source')
    return {'envelope':deepcopy(envelope),'combined':None,'native_body':deepcopy(leaf),
        'materialized_member_result':None,'member_body':leaf,'qualified_origins':[],
        'publication_identity':deepcopy(pub),'native_shape':'singleton'}


def observe(envelope,index,oracle,evidence,quality,*,kind,expected_publications=None,request_arguments=None,expected_scope=None):
    require(kind in ('search','diagnostics'),'unsupported observation purpose')
    require(len(evidence.canonical(envelope))<=1048576,'native evidence exceeds public envelope bound')
    body=canonical_body(envelope)
    if body.get('schema')=='airk.combined-read/1':
        if request_arguments is not None:
            require(isinstance(request_arguments,dict)
                and bool({'project_ids','index_ids','member_availability'} & request_arguments.keys()),
                'one-index combined response requires original explicit subset control')
        result=observe_combined(envelope,index,oracle,evidence,quality,kind=kind,
            expected_publications=expected_publications)
        require_scope(result['member_body'],index,oracle,quality,expected_scope)
        return result
    require('schema' not in body,'unknown canonical native response schema')
    return observe_singleton(envelope,index,oracle,evidence,quality,kind=kind,
        request_arguments=request_arguments,expected_publications=expected_publications,expected_scope=expected_scope)


def applied_focus(observation):
    focus = observation['member_body'].get('focus')
    return isinstance(focus, dict) and focus.get('kind') == 'receipt' and focus.get('receipt', {}).get('applied') is True


def require_applied_focus(observation, index, profile):
    require(applied_focus(observation), 'actual native Focus application absent')
    leaf = observation['member_body']; receipt = leaf['focus']['receipt']
    require(receipt['identity']['profile'] == profile and receipt['scope']['index_id'] == index
            and receipt['scope']['publication'] == leaf['publication']
            and receipt['scope']['authorization'] == leaf['scope_observation']['fingerprint']
            and receipt['scope']['configuration_revision'] > 0
            and receipt['candidate_count'] == len(leaf['hits'])
            and receipt['scored_count'] == len(receipt['scores']) > 0
            and receipt['observed_input_tokens'] > 0, 'Focus work/source binding differs')
    require(all(type(score[key]) in (float, int) and math.isfinite(score[key])
                for score in receipt['scores'] for key in ('no_logit', 'yes_logit', 'relevance')),
            'Focus scores are not finite')
    # Exact score normalization/order/timing are enforced by the native Search receipt owner.
    return receipt

def require_hybrid(observation):
    leaf = observation['member_body']
    require(leaf.get('hits') and leaf.get('execution', {}).get('served_mode') == 'hybrid'
            and leaf['execution'].get('incomplete') is False and leaf.get('truncated') is False,
            'complete native bundled MiniLM hybrid execution absent')


def require_health_source(health,sid,source_sha256):
    require(health.get('user') == sid, 'actual service human identity differs')
    build = health.get('build')
    require(isinstance(build,dict) and not health.get('build_error')
            and build.get('source_sha256') == source_sha256,
            'actual service build source unavailable or different: ' + str(health.get('build_error')))
