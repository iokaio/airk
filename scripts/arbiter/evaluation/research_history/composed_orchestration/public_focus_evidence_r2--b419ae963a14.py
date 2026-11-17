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


def observe(envelope, index, oracle, evidence, quality, *, kind, expected_publications=None):
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
            'qualified_origins': origins, 'publication_members': deepcopy(rows)}


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
            and leaf['execution'].get('incomplete') is False,
            'complete native bundled MiniLM hybrid execution absent')
