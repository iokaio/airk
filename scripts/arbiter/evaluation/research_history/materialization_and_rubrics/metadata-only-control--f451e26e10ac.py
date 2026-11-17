# SPDX-License-Identifier: Apache-2.0
"""Metadata hashes only; never opens raw questions, Qasper or gold payloads."""
import ast
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import threading
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def fingerprint(membership):
    digest = hashlib.sha256()
    for question in sorted(membership):
        component, articles = membership[question]
        digest.update(json.dumps([question, component, sorted(articles)], separators=(',', ':')).encode())
        digest.update(b'\n')
    return digest.hexdigest()


def main():
    assert sys.flags.isolated
    started = time.monotonic()
    previous = HERE.with_name('a1-fresh-final-materializer-r3')
    protocol = json.loads((previous / 'protocol.json').read_text(encoding='utf-8'))
    subject = load(HERE / 'adapter.py', 'metadata_control_subject')
    old_tree = ast.parse((previous / 'adapter.py').read_text(encoding='utf-8'))
    new_tree = ast.parse((HERE / 'adapter.py').read_text(encoding='utf-8'))
    for name in ('metadata_projection', 'final_membership', 'rank'):
        old = next(node for node in old_tree.body if isinstance(node, ast.FunctionDef) and node.name == name)
        new = next(node for node in new_tree.body if isinstance(node, ast.FunctionDef) and node.name == name)
        assert ast.dump(old) == ast.dump(new)
    owner = protocol['inputs']['audit_source']
    source = (ROOT / owner['path']).read_bytes()
    assert hashlib.sha256(source).hexdigest() == owner['sha256']
    budget = subject.import_bytes('metadata_only_budget', ROOT / owner['path'], source)
    budget.JOB_STATE = budget.enforce_budget()
    subject.MEMORY_OBSERVERS = (budget.peak_memory, None)
    def deadline():
        if not budget.done.wait(30):
            os._exit(124)
    threading.Thread(target=deadline, daemon=True).start()
    try:
        entry = protocol['inputs']['metadata']
        raw = (ROOT / entry['path']).read_bytes()
        assert len(raw) == entry['bytes'] and hashlib.sha256(raw).hexdigest() == entry['sha256']
        metadata = subject.metadata_projection(raw)
        del raw
        membership = subject.final_membership(metadata)
        before_hash = fingerprint(membership)
        before = subject.safe_memory_counters()
        subject.release_metadata_projection(metadata)
        del metadata
        after = subject.safe_memory_counters()
        assert fingerprint(membership) == before_hash
        assert len(membership) == 2396 and len({value[0] for value in membership.values()}) == 1755
        assert after['current_committed_process_bytes'] < before['current_committed_process_bytes']
        assert after['peak_committed_process_bytes'] <= 268435456
        report = {'schema': 'airk.private-final-metadata-memory-control/1', 'complete': True,
                  'metadata_sha256': entry['sha256'], 'same_r3_metadata_validation_and_rank_ast': True,
                  'membership_sha256_before_and_after': before_hash,
                  'final_questions': 2396, 'final_components': 1755,
                  'before_release': before, 'after_release': after,
                  'elapsed_seconds': time.monotonic() - started,
                  'hard_memory_limit_bytes': 268435456, 'hard_wall_seconds': 30,
                  'raw_questions_or_evidence_or_gold_opened': False,
                  'content_alignment_or_quality_qualification': False}
        output = HERE / 'metadata-only-control.json'
        with output.open('x', encoding='utf-8', newline='\n') as stream:
            json.dump(report, stream, indent=2)
            stream.write('\n')
        print(json.dumps(report))
    finally:
        budget.done.set()


if __name__ == '__main__':
    try:
        main()
    except BaseException:
        print('{"complete":false,"refusal":"metadata_only_control_failed"}')
        sys.exit(1)
