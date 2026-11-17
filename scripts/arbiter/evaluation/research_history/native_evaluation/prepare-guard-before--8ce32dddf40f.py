# SPDX-License-Identifier: Apache-2.0
"""Run only trusted trainer preprocessing AST; no model/Torch imports or weights."""
from pathlib import Path
import ast
import hashlib
import importlib.util
import json
import sys
import threading
import os
import time

START = time.monotonic()
HERE = Path(__file__).resolve().parent
BASE = HERE.parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / 'tools/retrieval-train'))
spec = importlib.util.spec_from_file_location('candidate_data_clock', BASE / 'a1-next-experiment-blind-metadata-r5/audit.py')
clock = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clock)
clock.START = START
clock.JOB_STATE = clock.enforce_budget()


def expire():
    if not clock.done.wait(max(0, 60 - (time.monotonic() - START))):
        os.write(2, b'REFUSED: candidate TRAIN preprocessing60second budget exhausted\n')
        os._exit(124)


threading.Thread(target=expire, daemon=True).start()
try:
    import training_recipe
    from tokenizers import Tokenizer
    seal_path = HERE / 'input-seal.json'
    seal = json.loads(seal_path.read_text(encoding='utf-8'))
    for row in seal['files']:
        if clock.digest(row['path']) != row['sha256']:
            raise ValueError('candidate preprocessing source/input drift')
    corpus = training_recipe.train_only_corpus((BASE / 'human-corpus-r1/corpus.private.json').read_bytes())
    extra = json.loads((BASE / 'requests-train-rights/pairs.private.json').read_text(encoding='utf-8'))
    if any(t['split'] != 'train' or t['dataset'] != 'requests-original-docstrings' for t in extra['tasks']):
        raise ValueError('Requests nonTRAIN task refused')
    corpus['tasks'].extend(extra['tasks'])
    corpus['documents'].extend(extra['documents'])
    source = (REPO / 'tools/retrieval-train/train_encoder.py').read_text(encoding='utf-8')
    tree = ast.parse(source)
    main = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'main')
    def assignment(node, name):
        return isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == name for t in node.targets)
    start = next(i for i, n in enumerate(main.body) if assignment(n, 'documents'))
    end = next(i for i, n in enumerate(main.body[start:], start) if isinstance(n, ast.If) and ast.unparse(n.test) == 'not examples') + 1
    body = ast.Module(body=main.body[start:end], type_ignores=[])
    # Source-bounded known data-only region. A future import or new call here refuses review.
    if any(isinstance(n, (ast.Import, ast.ImportFrom)) for n in ast.walk(body)):
        raise ValueError('unexpected import in data-only trainer region')
    for call in (n for n in ast.walk(body) if isinstance(n, ast.Call)):
        allowed = ((isinstance(call.func, ast.Name) and call.func.id in {'sorted', 'set', 'ValueError'}) or
                   (isinstance(call.func, ast.Attribute) and call.func.attr in {'lower', 'split', 'append', 'visible_prefix', 'negative_order'}))
        if not allowed:
            raise ValueError('unexpected call in data-only preprocessing region')
    code = compile(body, '<sealed-trainer-preprocessing-only>', 'exec')
    tokenizer = Tokenizer.from_file(str(BASE / 'minilm-training-source/tokenizer.json'))
    tokenizer.enable_truncation(max_length=256)
    tokenizer.enable_padding(pad_id=0, pad_token='[PAD]')
    tokenizer_before = tokenizer.to_str()
    default = {'corpus': corpus, 'tokenizer': tokenizer, 'recipe': None}
    exec(code, default)
    original_path = BASE / 'minilm-trained-r3/training-rows.private.json'
    original = json.loads(original_path.read_text(encoding='utf-8'))
    if default['examples'] != original or len(original) != 315:
        raise ValueError('old default315 training rows changed')
    candidate = {'corpus': corpus, 'tokenizer': tokenizer, 'recipe': training_recipe}
    exec(code, candidate)
    if tokenizer.to_str() != tokenizer_before:
        raise ValueError('candidate mining changed actual training tokenizer state')
    if len(candidate['examples']) != 315 or candidate['known_positives'] != default['known_positives']:
        raise ValueError('positive supervision/row cardinality changed')
    for old, new in zip(original, candidate['examples']):
        if any(old[key] != new[key] for key in ('task_id', 'question', 'positive')):
            raise ValueError('original positive/question/order changed')
    for row in seal['files']:
        if clock.digest(row['path']) != row['sha256']:
            raise ValueError('post-preprocessing source/input drift')
    clock.atomic_stream(HERE / 'candidate-training-rows.private.json', candidate['examples'])
    report = {'schema': 'airk.private-visible-prefix-candidate-preparation/1',
              'protocol_sha256': clock.digest(HERE / 'candidate-protocol.json'), 'input_seal_sha256': clock.digest(seal_path),
              'default_original_rows_sha256': clock.digest(original_path), 'default315_rows_parsed_identical': True,
              'candidate_rows_sha256': clock.digest(HERE / 'candidate-training-rows.private.json'),
              'candidate_rows': 315, 'positive_question_and_order_unchanged': True,
              'negative_parent_ids_changed': sum(a['distractor'] != z['distractor'] for a, z in zip(original, candidate['examples'])),
              'allowed_TRAIN_passages': len(candidate['allowed']), 'TRAIN_groups': len(candidate['train_groups']),
              'training_tokenizer_state_unchanged': True,
              'trusted_preprocessing_AST_sha256': hashlib.sha256(ast.dump(body, include_attributes=False).encode()).hexdigest(),
              'peak_committed_process_bytes': clock.peak_memory(), 'elapsed_seconds': time.monotonic() - START,
              'scope': 'Data preprocessing only; no Torch/model weights/NN/gradients/exports/scoring/native/Cargo/network; no held-out text decoded.'}
    clock.atomic_stream(HERE / 'preparation-receipt.json', report)
    print(json.dumps(report))
except Exception as error:
    print(json.dumps({'status': 'REFUSED', 'exception_type': type(error).__name__, 'reason': str(error),
                      'peak_committed_process_bytes': clock.peak_memory(), 'elapsed_seconds': time.monotonic() - START}), file=sys.stderr)
    raise
finally:
    clock.done.set()
