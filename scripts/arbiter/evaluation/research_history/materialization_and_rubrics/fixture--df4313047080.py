# SPDX-License-Identifier: Apache-2.0
"""Private FINAL mechanical source/range projection. No native wire DTO owner."""
from collections import Counter
import hashlib
from pathlib import PurePosixPath


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def require(condition):
    if not condition:
        raise ValueError('final_fixture_contract_refused')


def build(corpus, output, expected_counts=None):
    expected_counts = expected_counts or {'HotpotQA': 171, 'QASPER': 34}
    tasks = corpus['tasks']
    require(Counter(t['dataset'] for t in tasks) == expected_counts)
    require(len({t['id'] for t in tasks}) == len(tasks))
    require(all(t['split'] == 'test' and t['experiment_partition'] == 'FINAL' for t in tasks))
    docs = {d['id']: d for d in corpus['documents']}
    require(len(docs) == len(corpus['documents']))
    groups = {}
    for doc in docs.values():
        require(doc['dataset'] in expected_counts and sha(doc['text'].encode()) == doc['paragraph_sha256'])
        groups.setdefault(doc['source_group'], []).append(doc)
    bodies, files, paragraphs, mapping = {}, [], [], {}
    for group, documents in sorted(groups.items()):
        hotpot = documents[0]['dataset'] == 'HotpotQA'
        require(all(d['dataset'] == documents[0]['dataset'] for d in documents))
        if hotpot:
            require(len(documents) == 1)
            identifier = sha(group.encode())
            name = 'sources/hotpot/' + identifier + '.txt'
        else:
            require(group.startswith('qasper:'))
            identifier = group.split(':', 1)[1]
            require(identifier and '/' not in identifier and '\\' not in identifier and ':' not in identifier and identifier not in ('.', '..'))
            documents.sort(key=lambda d: tuple(int(part) for part in d['id'].split(':')[-2:]))
            name = 'sources/qasper/' + identifier + '/' + identifier + '.txt'
        require(not PurePosixPath(name).is_absolute() and '..' not in PurePosixPath(name).parts)
        pieces, local, offset, line = [], [], 0, 1
        for doc in documents:
            body = doc['text'].encode()
            if pieces:
                pieces.append(b'\n\n')
                offset += 2
                line += 2
            row = {'id': doc['id'], 'source_group': group, 'file': name,
                   'paragraph_sha256': sha(body), 'byte_start': offset, 'byte_end': offset + len(body),
                   'line_start': line, 'line_end': line + body.count(b'\n'),
                   'title': doc['title'], 'original_url': doc['url'], 'license': doc['license'],
                   'attribution': doc.get('attribution'), 'source_version_basis': doc.get('source_version_basis')}
            require(doc['id'] not in mapping)
            mapping[doc['id']] = row
            local.append(row)
            paragraphs.append(row)
            pieces.append(body)
            offset += len(body)
            line += body.count(b'\n')
        body = b''.join(pieces)
        bodies[name] = body
        files.append({'source_group': group, 'file': name, 'sha256': sha(body), 'bytes': len(body), 'paragraphs': local})
    selected_audit = [r for r in corpus['hotpot_alignment_audit_private'] if r['selected']]
    hotpot = {r['original_row']['id']: r for r in selected_audit}
    require(len(hotpot) == len(selected_audit) == expected_counts['HotpotQA'])
    require(set(hotpot) == {t['question_id'] for t in tasks if t['dataset'] == 'HotpotQA'})
    rubric = []
    for task in sorted(tasks, key=lambda t: t['id']):
        alternative_ids, alternatives = set(), []
        for original in task['human_alternatives']:
            require(original['human_annotation_id'] not in alternative_ids)
            alternative_ids.add(original['human_annotation_id'])
            require(type(original['original_evidence_count']) is int and original['original_evidence_count'] == len(original['evidence']))
            facts = []
            for fact in original['evidence']:
                locations = []
                require(len(set(fact['allowed_locations'])) == len(fact['allowed_locations']))
                for identifier in fact['allowed_locations']:
                    require(identifier in mapping)
                    paragraph = mapping[identifier]
                    start, end = paragraph['byte_start'], paragraph['byte_end']
                    if task['dataset'] == 'HotpotQA':
                        raw = hotpot[task['question_id']]['original_row']
                        require(raw['question'] == task['question'])
                        title = raw['context']['title'].index(fact['source_title'])
                        sentences = raw['context']['sentences'][title]
                        index = fact['sentence_index']
                        require(type(index) is int and 0 <= index < len(sentences))
                        sentence = sentences[index].encode()
                        require(sentence and sha(sentence) == fact['source_evidence_sha256'])
                        start += len(''.join(sentences[:index]).encode())
                        end = start + len(sentence)
                        require(bodies[paragraph['file']][start:end] == sentence)
                    else:
                        require(paragraph['source_group'] == task['source_group'])
                        require(sha(bodies[paragraph['file']][start:end]) == fact['source_evidence_sha256'])
                    require(start < end)
                    locations.append({'file': paragraph['file'], 'byte_start': start, 'byte_end': end,
                                      'original_url': paragraph['original_url'], 'source_group': paragraph['source_group']})
                facts.append({'original_evidence_sha256': fact.get('source_evidence_sha256'), 'locations': locations})
            alternatives.append({'human_annotation_id': original['human_annotation_id'], 'facts': facts,
                                 'original_human_answer': original['original_human_answer'],
                                 'unanswerable': original['unanswerable'], 'alignment': original['alignment'],
                                 'original_alternative': original})
        require(any(not a['unanswerable'] and a['facts'] and all(f['locations'] for f in a['facts'])
                    and a['alignment'] == 'all_original_evidence_maps_to_allowed_text' for a in alternatives))
        scope = output / ('sources/hotpot' if task['dataset'] == 'HotpotQA' else 'sources/qasper/' + task['source_group'].split(':', 1)[1])
        rubric.append({'id': task['id'], 'dataset': task['dataset'], 'question': task['question'],
                       'source_root': str(scope.resolve()), 'alternatives': alternatives,
                       'source_group': task['source_group'], 'disagreement': task['disagreement'],
                       'bootstrap_source_component': hotpot[task['question_id']]['component_digest'] if task['dataset'] == 'HotpotQA' else task['source_group']})
    # Existing A0 downstream ranking, mechanically frozen before any FINAL score.
    goal_ids = sorted((t['id'] for t in tasks), key=lambda value: sha(('airk.goal.final.v1:' + value).encode()))[:100]
    return bodies, {'schema': 'airk.private-native-human-fixtures/1', 'files': files,
                    'paragraphs': paragraphs, 'goal_ids': goal_ids, 'release_qualified': False}, rubric
