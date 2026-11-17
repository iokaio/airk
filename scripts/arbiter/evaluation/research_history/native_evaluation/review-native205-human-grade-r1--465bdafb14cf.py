# SPDX-License-Identifier: Apache-2.0
"""Self-review native205 human metrics with cached direct byte-set arithmetic.

No import of SourceLedger, goal_rubric, or grade_native_retrieval. This is a
separate computation by the same developer, not an independent human review.
"""
from datetime import datetime, timezone
from fractions import Fraction
import hashlib
import json
from pathlib import Path

base = Path(__file__).resolve().parent
paths = {
    'native': base/'native205-six-family-aggregate-r1.json',
    'graded': base/'native205-human-grade-r1/grading.private.json',
    'questions': base/'native-encoder-205-input-r1.json',
    'rubric': base/'human-goal-rubric-r4/rubric.private.json',
    'manifest': base/'native-human-fixtures-r2/manifest.private.json',
    'runner': Path(__file__),
}
retained = {name: path.read_bytes() for name, path in paths.items()}
loaded = {name: json.loads(raw) for name, raw in retained.items() if name != 'runner'}
fixture = paths['manifest'].parent
manifest = {row['file']: row for row in loaded['manifest']['files']}
questions = {row['id']: row for row in loaded['questions']['questions']}
tasks = {row['id']: row for row in loaded['rubric']['tasks'] if row['split'] == 'test'}
assert len(questions) == len(tasks) == 205 and set(questions) == set(tasks)
source_cache = {}; path_cache = {}; fact_cache = {}

def hit_span(hit, root):
    citation = hit['citation']; key = (root, citation['path'])
    if key not in path_cache:
        path = (Path(root)/citation['path']).resolve()
        path.relative_to(Path(root).resolve())
        relative = path.relative_to(fixture).as_posix()
        row = manifest[relative]
        if path not in source_cache:
            raw = path.read_bytes()
            assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256']
            source_cache[path] = raw
        path_cache[key] = path, relative, row
    path, relative, row = path_cache[key]
    assert citation['content_hash'] == row['sha256']
    lines = source_cache[path].splitlines(keepends=True)
    first, last = citation['start_line'], citation['end_line']
    assert 1 <= first <= last <= len(lines)
    needle = hit['text'].encode()
    if not needle:
        return None
    window = b''.join(lines[first-1:last]); at = window.find(needle)
    assert at >= 0 and window.find(needle, at+1) == -1
    begin = sum(map(len, lines[:first-1]))+at
    return relative, begin, begin+len(needle)

def covered(fact, bits):
    for location in fact['locations']:
        key = location['file'], location['byte_start'], location['byte_end']
        if key not in fact_cache:
            fact_cache[key] = set(range(key[1], key[2]))
        if fact_cache[key].issubset(bits.get(key[0], set())):
            return True
    return False

summary = {}
assert loaded['native']['complete'] is True and len(loaded['native']['families']) == 6
for family in loaded['native']['families']:
    name = family['family']; original = loaded['graded']['families'][name]
    rows = {row['id']: row for row in original['rows']}
    assert len(rows) == len(family['records']) == 205 and set(rows) == set(questions)
    mean = Fraction(0); complete = 0
    for record in family['records']:
        identifier = record['id']; task = tasks[identifier]; result = record['primary_vector']
        assert result['semantic_readiness']['state'] == 'ready'
        assert Path(record['source_root']).samefile(questions[identifier]['source_root'])
        bits = {}; spans = set()
        for hit in result['hits']:
            span = hit_span(hit, questions[identifier]['source_root'])
            if span is None:
                continue
            file, first, last = span
            bits.setdefault(file, set()).update(range(first, last)); spans.add(span)
        assert spans == {(row['file'], row['byte_start'], row['byte_end']) for row in rows[identifier]['source_spans']}
        alternatives = [Fraction(sum(covered(fact, bits) for fact in alt['facts']), len(alt['facts']))
                        if alt['facts'] else Fraction(0) for alt in task['alternatives']]
        best = max(alternatives, default=Fraction(0))
        assert Fraction(*rows[identifier]['evidence_recall_fraction']) == best
        assert rows[identifier]['complete_original_alternative'] == int(best == 1)
        mean += best; complete += best == 1
    mean /= 205
    assert Fraction(*original['mean_evidence_recall_fraction']) == mean and original['complete_count'] == complete
    summary[name] = {'mean_recall_fraction': [mean.numerator, mean.denominator],
                     'mean_recall': float(mean), 'complete_count': complete}
assert all(path.read_bytes() == retained[name] for name, path in paths.items())
assert all(path.read_bytes() == raw for path, raw in source_cache.items())
output = base/'native205-human-grade-byte-set-review-r1.json'
assert not output.exists()
report = {'schema': 'airk.private-native205-byte-set-self-review/1',
          'observed_at_utc': datetime.now(timezone.utc).isoformat(),
          'input_sha256': {name: hashlib.sha256(raw).hexdigest() for name, raw in retained.items()},
          'families': summary, 'all_1230_per_question_spans_and_scores_match': True,
          'method': 'Direct actual native citation/source proof and literal byte sets; preserve each original human alternative. No imports from production grader/projection helpers.',
          'scope': 'Independent arithmetic self-review by same developer. No new gold, tuning, repeated model evaluation, floor change, human action/authority judgment, or qualification claim.'}
output.write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
print(json.dumps({'all_1230_match': True, 'families': summary}))
