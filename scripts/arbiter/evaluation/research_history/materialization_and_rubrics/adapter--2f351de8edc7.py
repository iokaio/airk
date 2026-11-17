# SPDX-License-Identifier: Apache-2.0
"""Opaque FINAL-only neutral materialization; explicit reviewed execution required."""
import argparse
import ast
import base64
import ctypes
import gc
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import threading
import time
import types
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[4]
HERE = Path(__file__).resolve().parent
DOMAIN = b'airk.a1.next-experiment.v1/promotion-final\0'
LIMIT = 268435456
STAGE = 'metadata_preflight'


class OpaqueRefusal(ValueError):
    pass


def refuse(code):
    if code not in REFUSAL_CODES:
        raise RuntimeError('unregistered_refusal_code')
    raise OpaqueRefusal(code)


TRUSTED_CODES = {}
ARROW_EXCEPTION_CLASSES = ()
MEMORY_OBSERVERS = (None, None)
PHASE_MEMORY = []


def current_committed_bytes():
    if os.name != 'nt':
        return None
    from ctypes import wintypes
    class Counters(ctypes.Structure):
        _fields_ = [('cb', wintypes.DWORD), ('faults', wintypes.DWORD)] + [
            (name, ctypes.c_size_t) for name in ('peak_working_set', 'working_set',
            'peak_paged_pool', 'paged_pool', 'peak_nonpaged_pool', 'nonpaged_pool',
            'pagefile', 'peak_pagefile', 'private_usage')]
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    psapi = ctypes.WinDLL('psapi', use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD]
    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    if not psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(), ctypes.byref(counters), counters.cb):
        return None
    return counters.private_usage


def record_memory_phase(phase):
    if phase not in ('metadata_projected', 'metadata_membership_validated',
                     'metadata_released', 'arrow_ready', 'hotpot_materialized',
                     'scanner_created', 'batch_available', 'batch_projected'):
        return
    if any(item['phase'] == phase for item in PHASE_MEMORY):
        return
    event = {'phase': phase, **safe_memory_counters()}
    PHASE_MEMORY.append(event)
    # Immediate bounded numeric-only evidence survives an external timeout.
    print(json.dumps({'event': 'opaque_memory_phase', **event}), flush=True)


def release_metadata_projection(metadata):
    # All structural/global checks already ran in final_membership; it retains
    # exact FINAL strings/frozensets. Drop the otherwise unused complete graph.
    metadata.clear()
    gc.collect()



def bind_arrow_diagnostics(arrow):
    global ARROW_EXCEPTION_CLASSES
    # Explicit trusted library identities and static output labels; never names from data.
    ARROW_EXCEPTION_CLASSES = (
        (arrow.ArrowMemoryError, 'ArrowMemoryError'),
        (arrow.ArrowInvalid, 'ArrowInvalid'),
        (arrow.ArrowTypeError, 'ArrowTypeError'),
        (arrow.ArrowNotImplementedError, 'ArrowNotImplementedError'))


def safe_memory_counters():
    values = []
    for observer in MEMORY_OBSERVERS:
        value = None
        if observer is not None:
            try:
                observed = observer()
                if type(observed) is int and 0 <= observed <= 18446744073709551615:
                    value = observed
            except BaseException:
                pass  # Never expose observer messages, arguments or exception metadata.
        values.append(value)
    try:
        current = current_committed_bytes()
    except BaseException:
        current = None
    if type(current) is not int or not 0 <= current <= 18446744073709551615:
        current = None
    return {'peak_committed_process_bytes': values[0], 'arrow_current_pool_bytes': values[1],
            'current_committed_process_bytes': current}



def register_codes(code, filename):
    # Identity registration prevents data-generated code spoofing trusted filenames.
    TRUSTED_CODES[id(code)] = (code, filename, code.co_name)
    for child in code.co_consts:
        if isinstance(child, types.CodeType):
            register_codes(child, filename)


def safe_failure(error):
    allowed = (ValueError, TypeError, KeyError, IndexError, MemoryError,
               OSError, RuntimeError, AttributeError, OverflowError,
               UnicodeError, UnicodeDecodeError, UnicodeEncodeError, UnicodeTranslateError)
    kind = type(error).__name__ if type(error) in allowed else 'OtherException'
    for exception, label in ARROW_EXCEPTION_CLASSES:
        if type(error) is exception:
            kind = label
            break
    code = 'unexpected_exception'
    if type(error) is OpaqueRefusal and len(error.args) == 1:
        candidate = error.args[0]
        if type(candidate) is str and candidate in REFUSAL_CODES:
            code, kind = candidate, 'OpaqueRefusal'
    location = None
    trace = error.__traceback__
    while trace is not None:
        registered = TRUSTED_CODES.get(id(trace.tb_frame.f_code))
        if registered is not None and registered[0] is trace.tb_frame.f_code:
            location = {'filename': registered[1], 'function': registered[2], 'line': trace.tb_lineno}
        trace = trace.tb_next
    return {'complete': False, 'stage': STAGE,
            'refusal': 'opaque_final_materialization_refused',
            'diagnostic_code': code, 'exception_class': kind, 'location': location,
            'memory_counters': safe_memory_counters(), 'phase_memory_counters': list(PHASE_MEMORY)}


def sha(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def regular(path):
    path = Path(os.path.abspath(path))
    for ancestor in (path, *path.parents):
        item = ancestor.lstat()
        if stat.S_ISLNK(item.st_mode) or getattr(item, 'st_file_attributes', 0) & 1024:
            refuse('linked_input_or_parent')
    item = path.stat()
    if not stat.S_ISREG(item.st_mode) or item.st_nlink != 1:
        refuse('input_not_single_regular_file')
    return path


def rank(dataset, question):
    qualified = (dataset + ':' + question).encode('utf-8')
    return hashlib.sha256(DOMAIN + qualified).digest(), qualified


def skip_value(text, offset):
    """Skip unneeded metadata without materializing its large lineage map."""
    if text[offset] not in '[{':
        return json.JSONDecoder().raw_decode(text, offset)[1]
    depth, quoted, escaped = 0, False, False
    for index in range(offset, len(text)):
        char = text[index]
        if quoted:
            if escaped:
                escaped = False
            elif char == '\\':
                escaped = True
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
        elif char in '[{':
            depth += 1
        elif char in ']}':
            depth -= 1
            if depth == 0:
                return index + 1
    refuse('incomplete_metadata')


def metadata_projection(raw):
    if len(raw) > 16 * 1024 * 1024:
        refuse('metadata_size')
    text = raw.decode('utf-8')
    decoder, offset, projected, seen = json.JSONDecoder(), 1, {}, set()
    if not text.startswith('{'):
        refuse('metadata_shape')
    while True:
        while text[offset].isspace() or text[offset] == ',':
            offset += 1
        if text[offset] == '}':
            break
        key, offset = decoder.raw_decode(text, offset)
        if not isinstance(key, str) or key in seen:
            refuse('metadata_duplicate_key')
        seen.add(key)
        while text[offset].isspace():
            offset += 1
        if text[offset] != ':':
            refuse('metadata_separator')
        offset += 1
        while text[offset].isspace():
            offset += 1
        if key in ('schema', 'components', 'counts_before_unperformed_rights_alignment_checks'):
            projected[key], offset = decoder.raw_decode(text, offset)
        else:
            offset = skip_value(text, offset)
    return projected


def final_membership(metadata, expected_questions=2396, expected_components=1755):
    if metadata.get('schema') != 'airk.private-a1-blind-metadata-audit/1':
        refuse('metadata_schema')
    membership, seen_questions, seen_components = {}, set(), set()
    components = 0
    for component in metadata['components']:
        digest = component['component_digest']
        if len(digest) != 64 or len(bytes.fromhex(digest)) != 32 or digest in seen_components:
            refuse('component_identity')
        seen_components.add(digest)
        articles = frozenset(component['article_digests'])
        if len(articles) != len(component['article_digests']) or not articles:
            refuse('component_articles')
        for value in articles:
            if len(value) != 64 or len(bytes.fromhex(value)) != 32:
                refuse('article_identity')
        questions = component['question_ids']
        if not questions:
            refuse('empty_component')
        for question in questions:
            if not isinstance(question, str) or not question or question in seen_questions:
                refuse('question_identity')
            seen_questions.add(question)
            if component['disposition'] == 'FINAL_SEALED_UNOPENED':
                membership[question] = (digest, articles)
        if component['disposition'] == 'FINAL_SEALED_UNOPENED':
            components += 1
    if len(membership) != expected_questions or components != expected_components:
        refuse('final_population_changed')
    return membership


def load_neutral(raw):
    """Execute only exact owner pure function ASTs, never its old512/QASPER main."""
    tree = ast.parse(raw, filename='verified-human-corpus-owner')
    names = {'hash_bytes', 'append_hotpot_rows'}
    definitions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
    if {node.name for node in definitions} != names or len(definitions) != 2:
        refuse('neutral_owner_shape')
    module = types.ModuleType('verified_neutral_owner')
    module.__dict__.update(hashlib=hashlib, quote=quote)
    exec(compile(ast.Module(body=definitions, type_ignores=[]), 'verified-human-corpus-owner', 'exec'), module.__dict__)
    for name in names:
        register_codes(getattr(module, name).__code__, 'verified-human-corpus-owner')
    return module


def convert_hotpot(row, allowed_articles, article, neutral):
    qid = row['id']
    titles, sentences = row['context']['title'], row['context']['sentences']
    support = row['supporting_facts']
    if (not isinstance(qid, str) or not qid or not isinstance(row['question'], str)
            or not row['question'] or not isinstance(row['answer'], str)):
        refuse('publisher_question_answer_shape')
    if (not isinstance(titles, list) or not 1 <= len(titles) <= 10
            or len(titles) != len(sentences)):
        refuse('publisher_context_cardinality')
    # Same refusal boundary as R1, now separately diagnosable; no eligibility relaxation.
    if len(set(titles)) != len(titles):
        refuse('publisher_duplicate_context_titles')
    if any(not isinstance(title, str) or not title for title in titles):
        refuse('publisher_context_title_type')
    if any(not isinstance(parts, list) or any(not isinstance(part, str) for part in parts) for parts in sentences):
        refuse('publisher_sentence_sequence')
    if len(support['title']) != len(support['sent_id']):
        refuse('publisher_support_cardinality')
    if any(type(index) is not int or index < 0 for index in support['sent_id']):
        refuse('publisher_support_index')
    if not {article(title) for title in titles} <= allowed_articles:
        refuse('article_outside_captured_final_component')
    documents, tasks, exclusions = {}, [], []
    neutral.append_hotpot_rows([row], {qid}, documents, tasks, exclusions)
    if len(tasks) != 1 or tasks[0]['question_id'] != qid:
        refuse('neutral_task_identity')
    task = tasks[0]
    alternatives = task['human_alternatives']
    if (len(alternatives) != 1 or alternatives[0]['original_human_answer'] != row['answer']
            or alternatives[0]['original_evidence_count'] != len(support['title'])
            or len(alternatives[0]['evidence']) != len(support['title'])):
        refuse('original_support_or_answer_lost')
    alternative = alternatives[0]
    eligible = bool(alternative['original_evidence_count']) and alternative['alignment'] == 'all_original_evidence_maps_to_allowed_text'
    task['experiment_partition'] = 'FINAL'
    return task, documents, exclusions, eligible


def parquet_scan_options(dataset):
    # Pinned Arrow docs: bounded streaming avoids whole-row-group input buffers;
    # disable prebuffer's coalesced background reads on this local private file.
    return dataset.ParquetFragmentScanOptions(
        use_buffered_stream=True, buffer_size=8192, pre_buffer=False)


def filtered_rows(raw, wanted):
    global STAGE
    STAGE = 'filtered_arrow_import'
    import pyarrow.dataset as dataset
    STAGE = 'filtered_arrow_scan'
    scanner = dataset.dataset(raw, format='parquet').scanner(
        columns=['id', 'question', 'answer', 'context', 'supporting_facts'],
        filter=dataset.field('id').isin(wanted), batch_size=32,
        batch_readahead=0, fragment_readahead=0, use_threads=False,
        fragment_scan_options=parquet_scan_options(dataset))
    record_memory_phase('scanner_created')
    for batch in scanner.to_batches():
        record_memory_phase('batch_available')
        STAGE = 'filtered_arrow_projection'
        rows = batch.to_pylist()
        record_memory_phase('batch_projected')
        yield from rows


def select_hotpot(membership, rows, article, neutral, minimum=171):
    global STAGE
    ordered = sorted(membership, key=lambda qid: rank('hotpot', qid))
    documents, tasks, audit = {}, [], []
    counts = Counter({name: 0 for name in ('projected', 'validated', 'eligible_projected',
        'mechanically_refused_projected', 'considered_for_selection', 'selected', 'validated_unselected_tail')})
    for offset in range(0, len(ordered), 32):
        wanted = ordered[offset:offset + 32]
        captured = {}
        STAGE = 'filtered_final_rows'
        for row in rows(wanted):
            qid = row['id']
            if qid not in wanted or qid in captured:
                refuse('unselected_or_duplicate_filtered_row')
            captured[qid] = row
        if set(captured) != set(wanted):
            refuse('missing_filtered_final_row')
        # Validate and audit ALL captured rows, including the final batch's tail,
        # before returning a quota. Arrow order never determines selection.
        validated = []
        STAGE = 'filtered_final_row_validation'
        for qid in wanted:
            row = captured[qid]
            task, local_documents, exclusions, eligible = convert_hotpot(row, membership[qid][1], article, neutral)
            counts['projected'] += 1
            counts['validated'] += 1
            counts['eligible_projected' if eligible else 'mechanically_refused_projected'] += 1
            validated.append((qid, row, task, local_documents, exclusions, eligible))
        for qid, row, task, local_documents, exclusions, eligible in validated:
            considered = len(tasks) < minimum
            selected = considered and eligible
            counts['considered_for_selection' if considered else 'validated_unselected_tail'] += 1
            audit.append({'component_digest': membership[qid][0], 'original_row': row,
                          'complete_neutral_task': task, 'exclusions': exclusions,
                          'considered_for_selection': considered, 'selected': selected})
            if selected:
                for identifier, document in local_documents.items():
                    if identifier in documents and documents[identifier] != document:
                        refuse('cross_question_source_identity_conflict')
                    documents[identifier] = document
                tasks.append(task)
                counts['selected'] += 1
        if len(tasks) == minimum:
            return documents, tasks, audit, dict(counts)
    refuse('fewer_than_171_mechanically_eligible_final_questions')


def qasper_selection(payload, minimum=34, expected_raw=46, expected_eligible=38):
    if (payload.get('schema') != 'airk.private-qasper-fresh-final-sealed/1'
            or payload.get('minimum_met') is not True):
        refuse('sealed_qasper_shape')
    questions, eligible, annotations = {}, [], set()
    for paper in payload['papers']:
        for question in paper['questions']:
            qid = question['question_id']
            if qid in questions:
                refuse('duplicate_sealed_qasper_question')
            questions[qid] = (paper, question)
            alternatives = question['human_alternatives']
            for alternative in alternatives:
                original = alternative['original_human_record']
                if (alternative['human_annotation_id'] != original['annotation_id']
                        or original['annotation_id'] in annotations
                        or alternative['original_evidence_count'] != len(original['answer']['evidence'])
                        or len(alternative['evidence']) != alternative['original_evidence_count']):
                    refuse('sealed_qasper_original_support_lost')
                annotations.add(original['annotation_id'])
            usable = any(not alt['unanswerable'] and alt['original_evidence_count'] > 0
                         and alt['alignment'] == 'all_original_evidence_maps_to_allowed_text' for alt in alternatives)
            if (question['eligibility'] == 'eligible') != usable:
                refuse('sealed_qasper_eligibility_drift')
            if usable:
                eligible.append(qid)
    if len(questions) != expected_raw or len(eligible) != expected_eligible or len(eligible) < minimum:
        refuse('sealed_qasper_population_changed')
    selected = sorted(eligible, key=lambda qid: rank('qasper', qid))[:minimum]
    documents, tasks, rights = {}, [], []
    used_papers = set()
    for qid in selected:
        paper, question = questions[qid]
        decision = paper['decision']
        if paper['paper_id'] not in used_papers:
            used_papers.add(paper['paper_id'])
            rights.append(decision)
            for passage in paper['passages']:
                documents[passage['id']] = {**passage, 'dataset': 'QASPER', 'source_group': 'qasper:' + paper['paper_id'],
                    'title': decision['title'], 'license': decision['license'], 'url': 'https://arxiv.org/abs/' + paper['paper_id'],
                    'attribution': payload['attribution'], 'source_version_basis': decision['source_version_basis']}
        task = {key: value for key, value in question.items() if key != 'eligibility'}
        # Copies retain every complete publisher alternative; no new gold DTO.
        task = json.loads(json.dumps(task, ensure_ascii=False))
        for alternative in task['human_alternatives']:
            answer = alternative['original_human_record']['answer']
            alternative['original_human_answer'] = {key: answer[key] for key in ('unanswerable', 'extractive_spans', 'yes_no', 'free_form_answer')}
        task.update(id='qasper:' + qid, dataset='QASPER', split='test', publisher_split='test',
                    source_group='qasper:' + paper['paper_id'], experiment_partition='FINAL')
        tasks.append(task)
    return documents, tasks, rights


def import_bytes(name, path, raw):
    module = types.ModuleType(name)
    module.__file__ = str(path)
    exec(compile(raw, str(path), 'exec'), module.__dict__)
    return module


def anchor_observation(anchor, observer_raw):
    script = '& {\n' + observer_raw.decode('utf-8') + "\n} -Anchor '" + str(anchor).replace("'", "''") + "'"
    result = subprocess.run(['pwsh', '-NoProfile', '-NonInteractive', '-EncodedCommand',
        base64.b64encode(script.encode('utf-16-le')).decode('ascii')], capture_output=True, timeout=15)
    if result.returncode != 0:
        refuse('private_anchor_proof_failed')
    proof = json.loads(result.stdout)
    if (not proof.get('current_owner_only') or not proof.get('protected') or not proof.get('no_reparse')
            or not Path(proof['canonical_path']).samefile(anchor)):
        refuse('private_anchor_proof_invalid')
    return proof


def guard_output(anchor, output, protected, observer):
    anchor = Path(os.path.abspath(anchor))
    for part in (anchor, *anchor.parents):
        item = part.lstat()
        if stat.S_ISLNK(item.st_mode) or getattr(item, 'st_file_attributes', 0) & 1024:
            refuse('private_anchor_alias')
    if not anchor.is_dir() or list(anchor.iterdir()) or output.exists() or output.parent != anchor or output.name != 'materialized':
        refuse('fresh_empty_private_anchor_required')
    for path in protected:
        if output.resolve().is_relative_to(Path(path).resolve(strict=True)):
            refuse('output_inside_protected_input')
    before = anchor.stat()
    proof = observer(anchor)
    after = anchor.stat()
    if (before.st_dev, before.st_ino) != (after.st_dev, after.st_ino):
        refuse('anchor_identity_changed')
    return proof, (after.st_dev, after.st_ino)


def execute(args):
    global STAGE, MEMORY_OBSERVERS
    started = time.monotonic()
    if not sys.flags.isolated:
        refuse('isolated_runtime_required')
    protocol_raw = regular(args.protocol).read_bytes()
    if hashlib.sha256(protocol_raw).hexdigest() != args.protocol_sha256:
        refuse('protocol_hash_changed')
    protocol = json.loads(protocol_raw)
    seal_raw = regular(args.input_seal).read_bytes()
    if hashlib.sha256(seal_raw).hexdigest() != args.input_seal_sha256:
        refuse('input_seal_hash_changed')
    seal = json.loads(seal_raw)
    if seal['protocol_sha256'] != args.protocol_sha256 or protocol['schema'] != 'airk.private-fresh-final-materialization-protocol/1':
        refuse('foreign_protocol_or_seal')
    before = {}
    for row in seal['files']:
        path = regular(row['path'])
        if path in before or sha(path) != row['sha256']:
            refuse('input_source_runtime_drift')
        before[path] = (row['sha256'], path.stat().st_dev, path.stat().st_ino)
    for role, pin in protocol['inputs'].items():
        path = regular(ROOT / pin['path'])
        if path not in before or before[path][0] != pin['sha256']:
            refuse('missing_mandatory_protocol_input')
    if Path(__file__).resolve() not in before or Path(sys.executable).resolve() not in before:
        refuse('unsealed_adapter_or_interpreter')
    if str(Path(sys.executable).resolve()) != seal['runtime_executable']:
        refuse('wrong_interpreter')
    inputs = {role: ROOT / pin['path'] for role, pin in protocol['inputs'].items()}
    def verified_bytes(role):
        raw = inputs[role].read_bytes()
        if hashlib.sha256(raw).hexdigest() != protocol['inputs'][role]['sha256']:
            refuse('captured_input_bytes_changed')
        return raw
    # Import only authenticated pure/resource-owner code. The audit module's
    # __main__ graph/old512 projection is never invoked.
    budget = import_bytes('bounded_final_resource_owner', inputs['audit_source'], verified_bytes('audit_source'))
    if sys.version_info[:3] != (3, 13, 7) or budget.unicodedata.unidata_version != '15.1.0':
        refuse('frozen_interpreter_or_unicode_identity_changed')
    budget.START = started
    budget.JOB_STATE = budget.enforce_budget()
    MEMORY_OBSERVERS = (budget.peak_memory, None)
    def expire():
        if not budget.done.wait(max(0, 60 - (time.monotonic() - started))):
            os.write(2, b'opaque_final_resource_deadline\n')
            os._exit(124)
    threading.Thread(target=expire, daemon=True).start()
    try:
        anchor = Path(seal['output_anchor'])
        if anchor != ROOT / protocol['output_anchor']:
            refuse('frozen_output_anchor_changed')
        output = anchor / 'materialized'
        observer_raw = verified_bytes('anchor_observer')
        observe = lambda path: anchor_observation(path, observer_raw)
        protected = [ROOT / path for path in protocol['protected_input_roots']]
        proof, identity = guard_output(anchor, output, [*before, *protected], observe)
        STAGE = 'final_metadata_projection'
        metadata = metadata_projection(verified_bytes('metadata'))
        record_memory_phase('metadata_projected')
        membership = final_membership(metadata)
        record_memory_phase('metadata_membership_validated')
        release_metadata_projection(metadata)
        del metadata
        record_memory_phase('metadata_released')
        neutral = load_neutral(verified_bytes('neutral_owner'))
        for key in ('OMP_NUM_THREADS', 'OPENBLAS_NUM_THREADS', 'MKL_NUM_THREADS', 'NUMEXPR_NUM_THREADS', 'VECLIB_MAXIMUM_THREADS'):
            os.environ[key] = '1'
        import pyarrow
        bind_arrow_diagnostics(pyarrow)
        MEMORY_OBSERVERS = (budget.peak_memory, pyarrow.total_allocated_bytes)
        pyarrow.set_cpu_count(1)
        pyarrow.set_io_thread_count(1)
        if pyarrow.cpu_count() != 1 or pyarrow.io_thread_count() != 1:
            refuse('single_compute_thread_policy_failed')
        record_memory_phase('arrow_ready')
        STAGE = 'filtered_final_mechanical_alignment'
        documents, tasks, audit, counts = select_hotpot(membership,
            lambda wanted: filtered_rows(inputs['raw_hotpot'], wanted), budget.article, neutral)
        record_memory_phase('hotpot_materialized')
        STAGE = 'sealed_qasper_global_selection'
        # No developer/observer opens this gold. Preserve the existing R4 file.
        payload = json.loads(verified_bytes('sealed_qasper'))
        qdocs, qtasks, rights = qasper_selection(payload)
        if set(documents) & set(qdocs) or len({task['id'] for task in tasks + qtasks}) != 205:
            refuse('combined_final_identity_collision')
        documents.update(qdocs)
        corpus = {'schema': 'airk.private-human-evidence-corpus/1', 'documents': list(documents.values()),
            'tasks': tasks + qtasks, 'qasper_rights_decisions': rights,
            'hotpot_alignment_audit_private': audit,
            'qasper_rights_evidence_private': payload['rights_evidence'],
            'source_qasper_payload_sha256': protocol['inputs']['sealed_qasper']['sha256'],
            'qasper_historical_candidate_selection_unchanged': payload['selected_question_ids'],
            'protocol_sha256': args.protocol_sha256, 'input_seal_sha256': args.input_seal_sha256,
            'usage': 'OPAQUE FINAL EVALUATION ONLY; no training/calibration/threshold/quantizer/model use before separate native-owner qualification',
            'original205': 'FAILED unchanged; fresh composition is not a quality pass',
            'rights': 'HotpotQA/Wikipedia CC-BY-SA4; exact source URLs/attributions retained. QASPER exact previously approved per-paper rights and complete source grants retained; no new rights decision.'}
        STAGE = 'post_read_fences'
        for path, (digest, device, inode) in before.items():
            regular(path)
            if sha(path) != digest or (path.stat().st_dev, path.stat().st_ino) != (device, inode):
                refuse('input_changed_during_alignment')
        if sha(args.protocol) != args.protocol_sha256 or sha(args.input_seal) != args.input_seal_sha256:
            refuse('protocol_or_seal_changed')
        if (anchor.stat().st_dev, anchor.stat().st_ino) != identity or observe(anchor) != proof:
            refuse('anchor_changed_before_write')
        output.mkdir()
        STAGE = 'atomic_private_write'
        budget.atomic_stream(output / 'corpus.private.json', corpus)
        report = {'schema': 'airk.private-fresh-final-opaque-counts/1', 'complete': True,
            'hotpot_final_population': 2396, 'hotpot_final_components': 1755,
            'hotpot_selected': len(tasks), 'hotpot_alignment_counts': counts,
            'qasper_raw': 46, 'qasper_eligible': 38, 'qasper_selected': len(qtasks),
            'combined_tasks': len(tasks) + len(qtasks), 'documents': len(documents),
            'corpus_sha256': sha(output / 'corpus.private.json'), 'protocol_sha256': args.protocol_sha256,
            'input_seal_sha256': args.input_seal_sha256, 'anchor_proof_sha256': hashlib.sha256(json.dumps(proof, sort_keys=True).encode()).hexdigest(),
            'phase_memory_counters': list(PHASE_MEMORY),
            'peak_committed_process_bytes': budget.peak_memory(), 'elapsed_seconds': time.monotonic() - started,
            'compute_threads': 1, 'hard_memory_limit_bytes': LIMIT, 'hard_wall_seconds': 60,
            'original205_failed_unchanged': True, 'gold_opened_by_observer': False, 'quality_or_native_qualification': False}
        if report['elapsed_seconds'] >= 60 or report['peak_committed_process_bytes'] > LIMIT:
            refuse('resource_budget_exhausted')
        budget.atomic_stream(output / 'opaque-counts.json', report)
        print(json.dumps(report))
    finally:
        budget.done.set()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--protocol-sha256', required=True)
    parser.add_argument('--input-seal', type=Path, required=True)
    parser.add_argument('--input-seal-sha256', required=True)
    parser.add_argument('--execute-reviewed-content-alignment', action='store_true', required=True)
    args = parser.parse_args()
    try:
        execute(args)
    except BaseException as error:
        # No exception text, arguments, locals, chained exceptions, or source lines.
        print(json.dumps(safe_failure(error)))
        return 1
    return 0



REFUSAL_CODES = frozenset(('anchor_changed_before_write', 'anchor_identity_changed', 'article_identity', 'article_outside_captured_final_component', 'captured_input_bytes_changed', 'combined_final_identity_collision', 'component_articles', 'component_identity', 'cross_question_source_identity_conflict', 'duplicate_sealed_qasper_question', 'empty_component', 'fewer_than_171_mechanically_eligible_final_questions', 'final_population_changed', 'foreign_protocol_or_seal', 'fresh_empty_private_anchor_required', 'frozen_interpreter_or_unicode_identity_changed', 'frozen_output_anchor_changed', 'incomplete_metadata', 'input_changed_during_alignment', 'input_not_single_regular_file', 'input_seal_hash_changed', 'input_source_runtime_drift', 'isolated_runtime_required', 'linked_input_or_parent', 'metadata_duplicate_key', 'metadata_schema', 'metadata_separator', 'metadata_shape', 'metadata_size', 'missing_filtered_final_row', 'missing_mandatory_protocol_input', 'neutral_owner_shape', 'neutral_task_identity', 'original_support_or_answer_lost', 'output_inside_protected_input', 'private_anchor_alias', 'private_anchor_proof_failed', 'private_anchor_proof_invalid', 'protocol_hash_changed', 'protocol_or_seal_changed', 'publisher_context_cardinality', 'publisher_context_title_type', 'publisher_duplicate_context_titles', 'publisher_question_answer_shape', 'publisher_sentence_sequence', 'publisher_support_cardinality', 'publisher_support_index', 'question_identity', 'resource_budget_exhausted', 'sealed_qasper_eligibility_drift', 'sealed_qasper_original_support_lost', 'sealed_qasper_population_changed', 'sealed_qasper_shape', 'single_compute_thread_policy_failed', 'unsealed_adapter_or_interpreter', 'unselected_or_duplicate_filtered_row', 'wrong_interpreter') )
for _value in tuple(globals().values()):
    if isinstance(_value, types.FunctionType) and _value.__module__ == __name__:
        register_codes(_value.__code__, 'adapter.py')

if __name__ == '__main__':
    sys.exit(main())
