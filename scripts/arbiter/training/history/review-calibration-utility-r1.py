# SPDX-License-Identifier: Apache-2.0
"""Developer self-review using byte sets, independent of projection helpers."""
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path

base = Path(__file__).resolve().parent
paths = {
    'capture': base/'native-hybrid-calibration-capture-r1/native-output.json',
    'labels': base/'native-hybrid-calibration-labels-r1/labels.private.json',
    'requests': base/'native-calibration-input-r1.json',
    'rubric': base/'human-goal-rubric-r4/rubric.private.json',
    'manifest': base/'native-human-fixtures-r2/manifest.private.json',
    'runner': Path(__file__),
}
retained = {name: path.read_bytes() for name, path in paths.items()}
loaded = {name: json.loads(raw) for name, raw in retained.items() if name != 'runner'}
fixture = paths['manifest'].parent
sources = {}
manifest = {row['file']: row for row in loaded['manifest']['files']}
questions = {row['id']: row for row in loaded['requests']['questions']}
tasks = {row['id']: row for row in loaded['rubric']['tasks'] if row['split'] == 'calibration'}
labels = {row['id']: row for row in loaded['labels']['rows']}
assert loaded['capture']['read_population'] == 'trained-v2-native-default-hybrid'
assert len(questions) == len(labels) == len(loaded['capture']['rows']) == 19
assert set(questions) == set(labels) == {row['id'] for row in loaded['capture']['rows']}

def ranges(result, source_root):
    coverage = {}
    for hit in result.get('hits', []):
        citation = hit['citation']
        path = (Path(source_root)/citation['path']).resolve()
        path.relative_to(Path(source_root).resolve())
        relative = path.relative_to(fixture).as_posix()
        row = manifest[relative]
        raw = sources.setdefault(path, path.read_bytes())
        assert hashlib.sha256(raw).hexdigest() == row['sha256'] == citation['content_hash']
        assert len(raw) == row['bytes']
        lines = raw.splitlines(keepends=True)
        first, last = citation['start_line'], citation['end_line']
        assert 1 <= first <= last <= len(lines)
        window = b''.join(lines[first-1:last]); needle = hit['text'].encode()
        if not needle:
            continue
        offset = window.find(needle)
        assert offset >= 0 and window.find(needle, offset+1) == -1
        begin = sum(map(len, lines[:first-1]))+offset
        coverage.setdefault(relative, set()).update(range(begin, begin+len(needle)))
    return coverage

def count(alternative, coverage):
    return sum(any(set(range(location['byte_start'], location['byte_end'])).issubset(
        coverage.get(location['file'], set())) for location in fact['locations'])
        for fact in alternative['facts'])

operations = Counter(); useful = 0; prior_complete = 0; opportunities = 0
for episode in loaded['capture']['rows']:
    identifier = episode['id']; expected = labels[identifier]
    task = tasks[identifier]; question = questions[identifier]
    assert task['question'] == question['question'] == episode['request']['question']
    before = ranges(episode['before'], question['source_root'])
    original_counts = [count(alt, before) for alt in task['alternatives']]
    complete = any(bool(alt['facts']) and n == len(alt['facts'])
                   for alt, n in zip(task['alternatives'], original_counts))
    assert expected['before_any_original_alternative_complete'] == complete
    prior_complete += complete
    expected_choices = {row['choice_id']: row for row in expected['choices']}
    assert len(expected_choices) == len(episode['counterfactual_choices'])
    episode_useful = False
    for candidate in episode['counterfactual_choices']:
        operations[candidate['choice']['operation']] += 1
        after = {name: set(bits) for name, bits in before.items()}
        for name, bits in ranges(candidate['canonical_result'], question['source_root']).items():
            after.setdefault(name, set()).update(bits)
        projected = []
        for alternative, initial in zip(task['alternatives'], original_counts):
            following = count(alternative, after)
            projected.append({'human_annotation_id': alternative['human_annotation_id'],
                              'before_facts': initial, 'after_facts': following,
                              'adds_missing_original_support': following > initial})
        adds = any(value['adds_missing_original_support'] for value in projected)
        actual = expected_choices[candidate['choice']['id']]
        assert actual['original_alternative_projection'] == projected and actual['useful'] == adds
        useful += adds; episode_useful |= adds
    assert expected['has_useful_choice'] == episode_useful
    opportunities += episode_useful and not complete

assert all(path.read_bytes() == retained[name] for name, path in paths.items())
assert all(path.read_bytes() == raw for path, raw in sources.items())
output = base/'native-hybrid-calibration-utility-review-r1.json'
assert not output.exists()
report = {'schema': 'airk.private-calibration-utility-self-review/1',
          'observed_at_utc': datetime.now(timezone.utc).isoformat(),
          'input_sha256': {name: hashlib.sha256(raw).hexdigest() for name, raw in retained.items()},
          'episodes': 19, 'choices': sum(operations.values()), 'operations': dict(operations),
          'projected_useful_choices': useful, 'prior_complete_episodes': prior_complete,
          'missing_support_opportunity_episodes': opportunities, 'all_labels_match': True,
          'method': 'Separate literal-byte-set coverage computation, direct native hit/source checks; no imports of SourceLedger, goal_rubric, or calibration_labels. Each candidate restarts from identical prior read bytes; alternatives remain separate.',
          'authority': 'Developer self-review with independent computation, not an independent person or new human judgment. Original public human support mechanically projected; no authority/gap/sufficiency/probability or final quality claim.'}
output.write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
print(json.dumps({key: report[key] for key in ('episodes', 'choices', 'operations', 'projected_useful_choices', 'prior_complete_episodes', 'missing_support_opportunity_episodes', 'all_labels_match')}))
