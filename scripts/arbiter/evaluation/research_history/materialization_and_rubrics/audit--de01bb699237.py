# SPDX-License-Identifier: Apache-2.0
"""Private, sealed ID/title projection. Never imports corpus/model producers."""
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import sys
import threading
import time
import unicodedata

START = time.monotonic()
ROOT = Path(__file__).resolve().parent
BASE = ROOT.parent
LIMIT = 256 * 1024 * 1024
REV = '1908d6afbbead072334abe2965f91bd2709910ab'
done = threading.Event()


def expire():
    if not done.wait(60):
        os.write(2, b'REFUSED: hard wall-clock budget exhausted\n')
        os._exit(124)


class Basic(ctypes.Structure):
    _fields_ = [('process_time', ctypes.c_longlong), ('job_time', ctypes.c_longlong),
                ('flags', wintypes.DWORD), ('min_working_set', ctypes.c_size_t),
                ('max_working_set', ctypes.c_size_t), ('active', wintypes.DWORD),
                ('affinity', ctypes.c_size_t), ('priority', wintypes.DWORD),
                ('scheduling', wintypes.DWORD)]


class IO(ctypes.Structure):
    _fields_ = [(name, ctypes.c_ulonglong) for name in
                ('read_ops', 'write_ops', 'other_ops', 'read_bytes', 'write_bytes', 'other_bytes')]


class Extended(ctypes.Structure):
    _fields_ = [('basic', Basic), ('io', IO), ('process_memory', ctypes.c_size_t),
                ('job_memory', ctypes.c_size_t), ('peak_process', ctypes.c_size_t),
                ('peak_job', ctypes.c_size_t)]


def enforce_budget():
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.CreateJobObjectW.restype = wintypes.HANDLE
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.SetInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD]
    kernel.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
    job = kernel.CreateJobObjectW(None, None)
    limits = Extended()
    limits.basic.flags = 0x100  # JOB_OBJECT_LIMIT_PROCESS_MEMORY, hard committed-memory cap.
    limits.process_memory = LIMIT
    if not job or not kernel.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)):
        raise RuntimeError('cannot establish hard256MiB job memory cap')
    if not kernel.AssignProcessToJobObject(job, kernel.GetCurrentProcess()):
        raise RuntimeError('cannot assign audit process to bounded job')
    return kernel, job


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def canonical(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode('utf-8')


def article(title):
    if not isinstance(title, str) or not title:
        raise RuntimeError('invalid context title')
    normalized = unicodedata.normalize('NFC', title).replace('_', ' ')
    normalized = unicodedata.normalize('NFC', ' '.join(normalized.split()).casefold())
    payload = canonical(['HotpotQA-distractor', REV, 'enwiki', normalized])
    return hashlib.sha256(b'airk.a1.next-experiment.v1/article\0' + payload).hexdigest()


def main():
    kernel, job = enforce_budget()
    threading.Thread(target=expire, daemon=True).start()
    seal = load(ROOT / 'input-seal.json')
    for row in seal['files']:
        if digest(row['path']) != row['sha256']:
            raise RuntimeError('sealed input/runtime/source drift: ' + row['path'])
    if sys.version_info[:3] != (3, 13, 7) or unicodedata.unidata_version != '15.1.0':
        raise RuntimeError('protocol runtime mismatch')
    import pyarrow as arrow
    import pyarrow.parquet as parquet
    arrow.set_cpu_count(1)
    arrow.set_io_thread_count(1)
    seed = load(BASE / 'a1-next-experiment-owner-manifest-r1/old-hotpot-input-set-definition.json')
    old_ids = set(seed['original_512_ids'])
    if len(old_ids) != 512:
        raise RuntimeError('incomplete original512 seed roster')
    source = BASE / 'hotpot-dev.parquet'
    # Exact leaf selection: sentences, question, answer and supporting_facts are absent.
    table = parquet.read_table(source, columns=['id', 'context.title'], use_threads=False)
    if table.column_names != ['id', 'title'] or table.num_rows != 7405:
        raise RuntimeError('unexpected projected schema/count')
    if not arrow.types.is_list(table.schema.field('title').type) or not arrow.types.is_string(table.schema.field('title').type.value_type):
        raise RuntimeError('nested projection exposed an unapproved field')
    rows = table.to_pylist()
    del table
    ids = set()
    seed_articles = set()
    seed_slots = 0
    raw_title_digests = {}
    # Seed expansion MUST finish and validate before graph construction.
    for row in rows:
        qid = row['id']
        titles = row['title']
        if not isinstance(qid, str) or not qid or qid in ids or len(titles) != 10:
            raise RuntimeError('unexpected ID/context shape: ' + json.dumps({'question_id': qid, 'id_is_string': isinstance(qid, str), 'duplicate_id': qid in ids, 'context_title_count': len(titles)}))
        ids.add(qid)
        if qid in old_ids:
            for title in titles:
                a = article(title)
                seed_articles.add(a)
                raw_title_digests.setdefault(a, set()).add(hashlib.sha256(title.encode('utf-8')).hexdigest())
            seed_slots += len(titles)
    if not old_ids <= ids or seed_slots != 5120:
        raise RuntimeError('old512 ALL-context expansion incomplete')
    if not {article(title) for title in seed['known_allowed_title_families']} <= seed_articles:
        raise RuntimeError('retained known article title alias missing from seed expansion')
    parent = {qid: qid for qid in ids}

    def find(qid):
        while parent[qid] != qid:
            parent[qid] = parent[parent[qid]]
            qid = parent[qid]
        return qid

    first = {}
    articles_by_question = {}
    for row in rows:
        qid = row['id']
        aa = [article(title) for title in row['title']]
        articles_by_question[qid] = aa
        for title, a in zip(row['title'], aa):
            raw_title_digests.setdefault(a, set()).add(hashlib.sha256(title.encode('utf-8')).hexdigest())
            if a in first:
                parent[find(qid)] = find(first[a])
            else:
                first[a] = qid
    groups = {}
    for qid in ids:
        groups.setdefault(find(qid), []).append(qid)
    components = []
    counts = {name: {'questions': 0, 'article_families': 0, 'components': 0}
              for name in ['EXPOSED', 'DEV', 'CAL_SEALED_UNOPENED', 'FINAL_SEALED_UNOPENED']}
    for questions in groups.values():
        questions.sort(key=lambda x: x.encode('utf-8'))
        aa = sorted({a for q in questions for a in articles_by_question[q]}, key=lambda x: x.encode('utf-8'))
        component = hashlib.sha256(b'airk.a1.next-experiment.v1/component\0' + canonical([REV, questions, aa])).hexdigest()
        exposed = bool(set(questions) & old_ids or set(aa) & seed_articles)
        bucket = int.from_bytes(hashlib.sha256(b'airk.a1.next-experiment.v1/split\0' + bytes.fromhex(component)).digest(), 'big') % 10
        label = 'EXPOSED' if exposed else ('DEV' if bucket < 2 else 'CAL_SEALED_UNOPENED' if bucket < 4 else 'FINAL_SEALED_UNOPENED')
        count = counts[label]
        count['questions'] += len(questions)
        count['article_families'] += len(aa)
        count['components'] += 1
        components.append({'component_digest': component, 'question_ids': questions, 'article_digests': aa,
                           'bucket': bucket, 'disposition': label,
                           'exclusion': 'connected to original512/allcontext exposure' if exposed else None})
    limits = Extended()
    kernel.QueryInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD, ctypes.c_void_p]
    if not kernel.QueryInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits), None):
        raise RuntimeError('cannot observe peak job memory')
    final = counts['FINAL_SEALED_UNOPENED']
    result = {'schema': 'airk.private-a1-blind-metadata-audit/1', 'input_seal_sha256': digest(ROOT / 'input-seal.json'),
              'status': 'METADATA_ONLY_PENDING_PRIMARY_REVIEW_NO_ELIGIBILITY_CLAIM',
              'projected_columns': ['id', 'context.title'], 'projected_rows': len(rows),
              'seed_expansion': {'question_ids': 512, 'context_slots': seed_slots, 'unique_article_families': len(seed_articles),
                                 'retained_content_ids': len(seed['all_old_context_source_ids']),
                                 'content_ids_not_recomputed': 'Content IDs include sentence bytes. Exact rawasset+ID+ALLcontext selector closes seed scope without forbidden sentence projection.'},
              'counts_before_unperformed_rights_alignment_checks': counts,
              'metadata_floor_refusal': final['questions'] < 100 or final['article_families'] < 30,
              'rights_alignment_eligibility': 'NOT_PERFORMED; no eligible/cohort/model-use permission established',
              'components': sorted(components, key=lambda x: x['component_digest']),
              'article_normalization_lineage': {a: sorted(raw) for a, raw in sorted(raw_title_digests.items())},
              'peak_committed_process_bytes': limits.peak_process, 'hard_memory_limit_bytes': LIMIT,
              'elapsed_seconds': time.monotonic() - START, 'compute_threads': 1,
              'unchanged_failure': 'Original205 A1 remains FAILED. No rights/floor/metric/model/cohort change.'}
    if result['elapsed_seconds'] >= 60 or limits.peak_process > LIMIT:
        raise RuntimeError('resource budget exhausted')
    (ROOT / 'audit-result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    done.set()
    print(json.dumps({k: result[k] for k in ['status', 'projected_rows', 'seed_expansion', 'counts_before_unperformed_rights_alignment_checks', 'metadata_floor_refusal', 'peak_committed_process_bytes', 'elapsed_seconds']}))


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        done.set()
        print(json.dumps({'status': 'REFUSED', 'reason': str(error), 'elapsed_seconds': time.monotonic() - START}), file=sys.stderr)
        raise SystemExit(1)
