# SPDX-License-Identifier: Apache-2.0
"""Prepare private publisher facts, never native requests/rosters or model gold.

Only approved QASPER TRAIN and the exact original CAL question roster are decoded.
Requests is one TRAIN-only source family. This program performs no model work,
network access, source execution, native calls, or product qualification.
"""
import argparse
import ast
import copy
import doctest
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import tarfile
import sys

ROOT = next(directory for directory in Path(__file__).resolve().parents
            if (directory / 'Cargo.toml').is_file() and (directory / 'tools/retrieval-train/training_recipe_audit.py').is_file())
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / 'tools/retrieval-train'))
import training_recipe_audit as scanner
SCHEMA = 'airk.private-context-action-publisher-facts/1'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def fields(raw):
    """Read structural keys only; reject duplicate selectors BEFORE text decode."""
    at = scanner._white(raw, 0)
    if at >= len(raw) or raw[at] != 123:
        raise ValueError('JSON object required')
    result = {}
    at += 1
    while True:
        at = scanner._white(raw, at)
        if raw[at] == 125:
            return result
        end = scanner._string_end(raw, at)
        key = json.loads(raw[at:end])
        if key in result:
            raise ValueError('duplicate JSON field')
        at = scanner._white(raw, end)
        if raw[at] != 58:
            raise ValueError('JSON field colon missing')
        start = scanner._white(raw, at + 1)
        end = scanner._end(raw, start)
        result[key] = raw[start:end]
        at = scanner._white(raw, end)
        if raw[at] == 44:
            at += 1
        elif raw[at] != 125:
            raise ValueError('JSON object separator missing')


def raw_objects(raw):
    at = scanner._white(raw, 0)
    if raw[at] != 91:
        raise ValueError('JSON array required')
    at += 1
    while True:
        at = scanner._white(raw, at)
        if raw[at] == 93:
            return
        end = scanner._end(raw, at)
        yield raw[at:end]
        at = scanner._white(raw, end)
        if raw[at] == 44:
            at += 1
        elif raw[at] != 93:
            raise ValueError('JSON array separator missing')


def scalar(row, key):
    return json.loads(row[key])


def scoped_qasper(data, calibration_ids):
    """Cold scope selection: no Hotpot/TEST/DEV annotation or text deserialization."""
    top = fields(data)
    selected = []
    metadata = []
    for raw in raw_objects(top['tasks']):
        row = fields(raw)
        if scalar(row, 'dataset') != 'QASPER':
            continue
        split = scalar(row, 'split')
        if split not in ('train', 'calibration'):
            continue
        identifier = scalar(row, 'id')
        if split == 'calibration' and identifier not in calibration_ids:
            continue
        group = scalar(row, 'source_group')
        if not isinstance(group, str) or not group.startswith('qasper:'):
            raise ValueError('foreign QASPER family')
        selected.append(raw)
        metadata.append((identifier, split, group))
    if len({row[0] for row in metadata}) != len(metadata):
        raise ValueError('duplicate selected QASPER task')
    if {identifier for identifier, split, _ in metadata if split == 'calibration'} != set(calibration_ids):
        raise ValueError('original CAL question roster differs')
    groups = {split: {group for _, own, group in metadata if own == split} for split in ('train', 'calibration')}
    if groups['train'] & groups['calibration']:
        raise ValueError('TRAIN/CAL paper-family overlap')
    tasks = [json.loads(raw) for raw in selected]
    documents = []
    for raw in raw_objects(top['documents']):
        row = fields(raw)
        if scalar(row, 'dataset') == 'QASPER' and scalar(row, 'source_group') in groups['train'] | groups['calibration']:
            documents.append(json.loads(raw))
    papers = {group.removeprefix('qasper:') for group in groups['train'] | groups['calibration']}
    rights = [json.loads(raw) for raw in raw_objects(top['qasper_rights_decisions']) if scalar(fields(raw), 'paper_id') in papers]
    if {row['paper_id'] for row in rights} != papers or len(rights) != len(papers):
        raise ValueError('selected QASPER rights closure missing/ambiguous')
    for right in rights:
        if right['license'] != 'CC-BY-4.0' or not right.get('attribution'):
            raise ValueError('QASPER source rights differ')
    by_id = {doc['id']: doc for doc in documents}
    if len(by_id) != len(documents):
        raise ValueError('duplicate selected publisher paragraph')
    for doc in documents:
        if doc['license'] != 'CC-BY-4.0' or sha(doc['text'].encode()) != doc['paragraph_sha256']:
            raise ValueError('selected publisher paragraph identity differs')
    for task in tasks:
        expected_publisher = 'train' if task['split'] == 'train' else 'validation'
        if task['publisher_split'] != expected_publisher:
            raise ValueError('publisher split differs')
        for alternative in task['human_alternatives']:
            for requirement in alternative['evidence']:
                for identifier in requirement['allowed_locations']:
                    doc = by_id.get(identifier)
                    if not doc or doc['source_group'] != task['source_group'] or doc['paragraph_sha256'] != requirement['source_evidence_sha256']:
                        raise ValueError('human evidence points outside its exact allowed paper/text')
    return tasks, documents, rights


def reference_fact(alternative, shown, added=()):
    """Mechanical paragraph-reference completion, separately for ONE annotator."""
    evidence = alternative['evidence']
    admitted = (not alternative['unanswerable'] and evidence
                and alternative['alignment'] == 'all_original_evidence_maps_to_allowed_text'
                and all(item['allowed_locations'] for item in evidence))
    if not admitted:
        return {'annotation_id': alternative['human_annotation_id'], 'reference_complete_before': None,
                'reference_complete_after': None, 'adds_missing_reference_support': None,
                'all_shown_paragraphs_are_this_alternatives_support': None}
    shown, after = set(shown), set(shown) | set(added)
    covered = lambda visible, item: bool(visible & set(item['allowed_locations']))
    support = {identifier for item in evidence for identifier in item['allowed_locations']}
    return {'annotation_id': alternative['human_annotation_id'],
            'reference_complete_before': all(covered(shown, item) for item in evidence),
            'reference_complete_after': all(covered(after, item) for item in evidence),
            'adds_missing_reference_support': any(not covered(shown, item) and covered(after, item) for item in evidence),
            'all_shown_paragraphs_are_this_alternatives_support': bool(shown) and shown <= support}


def question_controls(task, documents, max_controls=4, max_probes=4):
    """Workload controls, NOT native choices, unique card gold or model inputs."""
    own = sorted(doc['id'] for doc in documents.values() if doc['source_group'] == task['source_group'])
    alternatives = task['human_alternatives']
    eligible = [alt for alt in alternatives if reference_fact(alt, [])['reference_complete_before'] is not None]
    states = [()]
    if eligible:
        first = min(eligible, key=lambda alt: str(alt['human_annotation_id']))
        complete = tuple(sorted({min(item['allowed_locations']) for item in first['evidence']}))
        states += [complete, complete[1:]]
        # Union is ONLY an exclusion set for this control, never an answer/support union.
        annotated = {loc for alt in alternatives for item in alt['evidence'] for loc in item['allowed_locations']}
        unannotated = next((identifier for identifier in own if identifier not in annotated), None)
        if unannotated:
            states.append(tuple(sorted(set(complete) | {unannotated})))
    elif own:
        states.append((own[0],))
    states = list(dict.fromkeys(states))[:max_controls]
    controls = []
    for shown in states:
        facts = [reference_fact(alt, shown) for alt in alternatives]
        missing = sorted({loc for alt in eligible for item in alt['evidence']
                          if not set(shown) & set(item['allowed_locations']) for loc in item['allowed_locations']})
        probes = missing[:max_probes]
        controls.append({'shown_full_paragraphs': [{'document_id': identifier, 'start_byte': 0,
                         'end_byte': len(documents[identifier]['text'].encode())} for identifier in shown],
                         'per_original_alternative': facts,
                         'reference_complete_alternatives': [fact['annotation_id'] for fact in facts if fact['reference_complete_before']],
                         'document_useful_whole_hypothesis_alternatives': [fact['annotation_id'] for fact in facts
                          if fact['reference_complete_before'] and fact['all_shown_paragraphs_are_this_alternatives_support']],
                         'additional_full_paragraph_probes': [{'document_id': identifier,
                          'per_original_alternative': [reference_fact(alt, shown, [identifier]) for alt in alternatives]} for identifier in probes],
                         'unprobed_missing_paragraphs': len(missing) - len(probes)})
    return controls


def structural_facts(path, raw):
    """Independent Python AST/doctest predicates. No import/eval/doctest execution."""
    text = raw.decode('utf-8-sig')
    tree = ast.parse(text, filename=path)
    lines = text.splitlines(keepends=True)
    starts = [3 if raw.startswith(b'\xef\xbb\xbf') else 0]
    for line in lines:
        starts.append(starts[-1] + len(line.encode()))
    def point(node, end=False):
        return starts[(node.end_lineno if end else node.lineno)-1] + (node.end_col_offset if end else node.col_offset)
    def span(node):
        return {'start_byte': point(node), 'end_byte': point(node, True), 'start_line': node.lineno, 'end_line': node.end_lineno}
    imports = {}
    for node in tree.body:
        if isinstance(node, ast.Import):
            for alias in node.names:
                if alias.name in ('pytest', 'unittest'):
                    imports.setdefault(alias.asname or alias.name, []).append((alias.name, node))
        if isinstance(node, ast.ImportFrom) and node.module == 'unittest':
            for alias in node.names:
                if alias.name == 'TestCase':
                    imports.setdefault(alias.asname or alias.name, []).append(('unittest.TestCase', node))
    shadowed = {node.id for node in ast.walk(tree) if isinstance(node, ast.Name) and isinstance(node.ctx, (ast.Store, ast.Del))}
    # Do not call a monkey-patched/shadowed alias a resolved framework binding.
    for node in ast.walk(tree):
        if isinstance(node, ast.Attribute) and isinstance(node.ctx, (ast.Store, ast.Del)):
            root = node.value
            while isinstance(root, ast.Attribute):
                root = root.value
            if isinstance(root, ast.Name):
                shadowed.add(root.id)
        if isinstance(node, (ast.Import, ast.ImportFrom)) and node not in tree.body:
            shadowed.update(alias.asname or alias.name.split('.')[0] for alias in node.names)
        if isinstance(node, ast.ImportFrom) and any(alias.name == '*' for alias in node.names):
            shadowed.update(imports)
    shadowed |= {node.arg for node in ast.walk(tree) if isinstance(node, ast.arg)}
    shadowed |= {node.name for node in ast.walk(tree) if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef))}
    def resolved(node):
        parts = []
        while isinstance(node, ast.Attribute):
            parts.insert(0, node.attr); node = node.value
        if not isinstance(node, ast.Name) or node.id in shadowed or len(imports.get(node.id, [])) != 1:
            return None
        name, binding = imports[node.id][0]
        return '.'.join([name] + parts), binding
    result = []
    def visit(nodes, parents=(), test_class=None):
        for node in nodes:
            if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                visit(ast.iter_child_nodes(node), parents, test_class)
                continue
            qualified = '.'.join((*parents, node.name))
            header = {'start_byte': point(node), 'end_byte': point(node.body[0]),
                      'start_line': node.lineno, 'end_line': node.body[0].lineno}
            base = {'source_file': path, 'source_file_sha256': sha(raw), 'qualified_declaration': qualified,
                    'ast_kind': type(node).__name__, 'whole_declaration': span(node)}
            result.append({**base, 'predicate': 'python_declaration_header', 'required_ranges': [header],
                           'limits': 'Written declaration identity only; no dynamic name resolution or goal relevance.'})
            case = None
            if isinstance(node, ast.ClassDef):
                cases = [resolved(base) for base in node.bases]
                case = next((value for value in cases if value and value[0] == 'unittest.TestCase'), None)
            elif test_class and node.name.startswith('test'):
                result.append({**base, 'predicate': 'unittest_testcase_method',
                               'required_ranges': [span(test_class[1]), test_class[2], header],
                               'limits': 'Static unittest TestCase construct; no execution/pass/coverage claim.'})
            if not isinstance(node, ast.ClassDef):
                for decorator in node.decorator_list:
                    subject = decorator.func if isinstance(decorator, ast.Call) else decorator
                    binding = resolved(subject)
                    decorator_span = span(decorator)
                    line_start = starts[decorator.lineno - 1]
                    line_prefix = raw[line_start:decorator_span['start_byte']]
                    marker = line_start + len(line_prefix) - len(line_prefix.lstrip(b' \t'))
                    explicit_marker = raw[marker:marker + 1] == b'@'
                    decorator_span['start_byte'] = marker
                    if binding and binding[0].startswith('pytest.mark.') and explicit_marker:
                        result.append({**base, 'predicate': 'explicit_pytest_mark_annotation',
                                       'required_ranges': [span(binding[1]), decorator_span, header],
                                       'limits': 'Explicit framework annotation only; no execution or collection guarantee.'})
            if node.body and isinstance(node.body[0], ast.Expr) and isinstance(node.body[0].value, ast.Constant) and isinstance(node.body[0].value.value, str):
                doc = node.body[0].value.value
                examples = doctest.DocTestParser().get_examples(doc)
                if examples:
                    result.append({**base, 'predicate': 'doctest_example_grammar', 'required_ranges': [span(node.body[0])],
                                   'decoded_docstring_sha256': sha(doc.encode()),
                                   'examples': [{'source': example.source, 'expected_output': example.want, 'docstring_line': example.lineno} for example in examples],
                                   'limits': 'Parsed original example/expected-output grammar; never executed or certified correct.'})
            visit(node.body, (*parents, node.name), (case[0], case[1], header) if case else None)
    visit(tree.body)
    return result


def read_pinned(descriptor):
    path = ROOT / descriptor['path']
    with path.open('rb') as stream:
        data = stream.read(descriptor['bytes'] + 1)
    if len(data) != descriptor['bytes'] or sha(data) != descriptor['sha256']:
        raise ValueError('reviewed input length/hash differs: ' + descriptor['path'])
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--protocol-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    protocol_bytes = args.protocol.read_bytes()
    if sha(protocol_bytes) != args.protocol_sha256:
        raise ValueError('protocol seal differs')
    protocol = json.loads(protocol_bytes)
    if protocol['schema'] != 'airk.private-context-action-facts-protocol/1':
        raise ValueError('unsupported data-only protocol')
    if sha(Path(__file__).read_bytes()) != protocol['producer_sha256'] or sha(Path(scanner.__file__).read_bytes()) != protocol['scanner_sha256']:
        raise ValueError('reviewed producer/scanner source differs')
    destination = args.output.resolve()
    if destination.exists() or not destination.is_relative_to(ROOT / 'artifacts' / 'advanced-split'):
        raise ValueError('fresh private advanced-split output required')
    inputs = {name: read_pinned(descriptor) for name, descriptor in protocol['inputs'].items()}
    roster = json.loads(inputs['calibration_roster'])
    calibration_ids = {row['id'] for row in roster['questions']}
    if len(calibration_ids) != len(roster['questions']):
        raise ValueError('duplicate original CAL ID')
    tasks, documents, rights = scoped_qasper(inputs['qasper_corpus'], calibration_ids)
    calibration_questions = {row['id']: row['question'] for row in roster['questions']}
    if any(task['question'] != calibration_questions[task['id']] for task in tasks if task['split'] == 'calibration'):
        raise ValueError('original CAL question bytes differ')
    counts = {'train_questions': sum(task['split'] == 'train' for task in tasks), 'calibration_questions': len(calibration_ids),
              'train_families': len({task['source_group'] for task in tasks if task['split'] == 'train'}),
              'calibration_families': len({task['source_group'] for task in tasks if task['split'] == 'calibration'})}
    if counts != protocol['expected_qasper_counts']:
        raise ValueError('reviewed TRAIN/CAL population differs')
    documentation = json.loads(inputs['requests_pairs'])
    commit = json.loads(inputs['requests_commit'])['sha']
    if documentation['revision'] != commit or sha(inputs['requests_archive']) != documentation['archive_sha256']:
        raise ValueError('Requests historical closure differs')
    if any(task['split'] != 'train' or task['dataset'] != 'requests-original-docstrings' for task in documentation['tasks']):
        raise ValueError('Requests must stay TRAIN-only')
    expected = {row['path']: row['sha256'] for row in documentation['source_files']}
    if not expected or len(expected) != len(documentation['source_files']):
        raise ValueError('empty/duplicate Requests source roster')
    for name in ('LICENSE', 'NOTICE'):
        declared = [item for item in documentation['license_artifacts'] if item['file'] == name]
        actual = inputs['requests_' + name.lower()]
        if len(declared) != 1 or declared[0]['sha256'] != sha(actual) or declared[0]['text'].encode() != actual:
            raise ValueError('Requests legal closure differs')
    facts, source_documents, unsupported, seen, legal_seen = [], [], [], set(), set()
    with tarfile.open(fileobj=io.BytesIO(inputs['requests_archive']), mode='r:gz') as archive:
        for member in archive:
            parts = PurePosixPath(member.name).parts
            relative = '/'.join(parts[1:])
            if relative in ('LICENSE', 'NOTICE'):
                if relative in legal_seen or not member.isfile() or archive.extractfile(member).read() != inputs['requests_' + relative.lower()]:
                    raise ValueError('historical archive legal file differs')
                legal_seen.add(relative)
            if relative not in expected:
                continue
            if relative in seen or not member.isfile() or member.size > protocol['max_python_file_bytes'] or '..' in parts:
                raise ValueError('unsafe/duplicate historical source member')
            seen.add(relative)
            raw = archive.extractfile(member).read()
            if sha(raw) != expected[relative]:
                raise ValueError('Requests source file differs')
            try:
                facts.extend(structural_facts(relative, raw))
                source_documents.append({'source_file': relative, 'source_file_sha256': sha(raw), 'text': raw.decode('utf-8')})
            except (SyntaxError, UnicodeDecodeError, ValueError) as error:
                unsupported.append({'path': relative, 'sha256': sha(raw), 'reason': type(error).__name__})
    if seen != set(expected) or legal_seen != {'LICENSE', 'NOTICE'} or len(facts) > protocol['max_structural_facts']:
        raise ValueError('Requests source roster or workload differs')
    by_id = {doc['id']: doc for doc in documents}
    questions = []
    for task in tasks:
        alternatives = [{key: copy.deepcopy(value) for key, value in alt.items() if key != 'original_human_answer'} for alt in task['human_alternatives']]
        questions.append({key: copy.deepcopy(task[key]) for key in ('id', 'question', 'source_group', 'split', 'publisher_split', 'dataset')} | {
            'human_alternatives': alternatives,
            'original_answerability_disagreement': task.get('disagreement'),
            'unanimously_human_unanswerable': bool(alternatives) and all(alt['unanswerable'] for alt in alternatives),
            'controls': question_controls(task, by_id, protocol['max_controls_per_question'], protocol['max_probe_paragraphs'])})
    result = {'schema': SCHEMA, 'questions': questions, 'documents': documents, 'rights': rights,
              'requests': {'split': 'train', 'revision': commit, 'source_family': 'psf/requests@' + commit,
                           'documents': source_documents, 'facts': facts, 'unsupported_files': unsupported},
              'authority': 'Mechanical human-reference completion/complement and source predicates, NOT new human action/gap/role/sufficiency gold.',
              'limitations': protocol['limitations']}
    # Input/outcome seal is written BEFORE any derived facts; this is not a model run.
    seal = {'schema': 'airk.private-context-action-preparation-seal/1', 'protocol_sha256': sha(protocol_bytes),
            'producer_sha256': protocol['producer_sha256'], 'scanner_sha256': protocol['scanner_sha256'],
            'inputs': protocol['inputs'], 'selected_task_ids': sorted(task['id'] for task in tasks), 'counts': counts}
    for name, descriptor in protocol['inputs'].items():
        if read_pinned(descriptor) != inputs[name]:
            raise ValueError('source drift before preparation')
    destination.mkdir(parents=True)
    for name, value in [('input-seal.json', seal), ('publisher-facts.private.json', result), ('original-annotations.private.json', {'tasks': tasks})]:
        with (destination / name).open('x', encoding='utf-8') as stream:
            json.dump(value, stream, ensure_ascii=False, indent=2); stream.write('\n')
    for name in ('requests_license', 'requests_notice'):
        with (destination / ('Requests-' + name.removeprefix('requests_').upper())).open('xb') as stream:
            stream.write(inputs[name])
    for name, descriptor in protocol['inputs'].items():
        if read_pinned(descriptor) != inputs[name]:
            raise ValueError('source drift after preparation')
    receipt = {'schema': 'airk.private-context-action-preparation/1', 'counts': counts,
               'structural_facts': len(facts), 'model_execution': False, 'qualified_profiles': [],
               'outputs': {path.name: {'bytes': path.stat().st_size, 'sha256': sha(path.read_bytes())} for path in destination.iterdir() if path.is_file()}}
    with (destination / 'receipt.json').open('x', encoding='utf-8') as stream:
        stream.write(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    main()
