# SPDX-License-Identifier: Apache-2.0
"""Primary-only owned60s opaque fixture preparation. Never parses FINAL content."""
import argparse
import hashlib
import json
from pathlib import Path
import stat
import sys
import types

ROOT = Path.cwd().resolve()
HERE = Path(__file__).resolve().parent


def require(condition):
    if not condition:
        raise SystemExit('reviewed_final_fixture_preflight_refused')


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def regular(path):
    path = Path(path).absolute()
    for candidate in (path, *path.parents):
        item = candidate.lstat()
        require(not stat.S_ISLNK(item.st_mode) and not getattr(item, 'st_file_attributes', 0) & 1024)
    item = path.stat()
    require(stat.S_ISREG(item.st_mode) and item.st_nlink == 1)
    return path


def capture(path):
    path = regular(path)
    before = path.stat()
    digest = sha(path)
    after = path.stat()
    require((before.st_dev, before.st_ino, before.st_size) == (after.st_dev, after.st_ino, after.st_size))
    return digest, after.st_dev, after.st_ino, after.st_size


def import_retained(name, path, raw):
    module = types.ModuleType(name)
    module.__file__ = str(path)
    exec(compile(raw, str(path), 'exec'), module.__dict__)
    return module


def strict_normal_close(owner, result):
    return owner.successful(result) and result.get('owned_job_forced_termination') is False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input-seal', type=Path, required=True)
    parser.add_argument('--input-seal-sha256', required=True)
    parser.add_argument('--protocol-sha256', required=True)
    parser.add_argument('--job-helper-sha256', required=True)
    parser.add_argument('--execution-receipt', type=Path, required=True)
    parser.add_argument('--execute-reviewed-opaque-fixture-preparation', action='store_true', required=True)
    args = parser.parse_args()
    require(sys.flags.isolated)
    protocol_path = regular(HERE / 'protocol.json')
    seal_path = regular(args.input_seal)
    protocol_raw, seal_raw = protocol_path.read_bytes(), seal_path.read_bytes()
    require(hashlib.sha256(protocol_raw).hexdigest() == args.protocol_sha256)
    require(hashlib.sha256(seal_raw).hexdigest() == args.input_seal_sha256)
    protocol, seal = json.loads(protocol_raw), json.loads(seal_raw)
    require(seal['schema'] == 'airk.private-fresh-final-fixture-input-seal/1')
    require(seal['protocol_sha256'] == args.protocol_sha256)
    require(protocol['schema'] == 'airk.private-fresh-final-evaluation-protocol/1')
    require(args.execution_receipt.parent.resolve() == HERE and not args.execution_receipt.exists())
    log = args.execution_receipt.with_suffix('.log')
    require(not log.exists())
    before, declared = {}, {}
    for row in seal['files']:
        path = regular(row['path'])
        require(path not in before)
        before[path] = capture(path)
        declared[path] = row['sha256']
        require(before[path][0] == row['sha256'])
    require(protocol_path in before and Path(__file__).resolve() in before)
    require(Path(sys.executable).resolve() == Path(seal['runtime_executable']).resolve())
    require(Path(seal['runtime_executable']).resolve() in before)
    for row in protocol['source_owners']:
        require(declared.get(ROOT / row['path']) == row['sha256'])
    corpus_path = ROOT / protocol['corpus']['path']
    require(declared.get(corpus_path) == protocol['corpus']['sha256'])
    helper_path = regular(ROOT / 'scripts/provision-static-onnx-runtime.py')
    helper_raw = helper_path.read_bytes()
    require(hashlib.sha256(helper_raw).hexdigest() == args.job_helper_sha256 == declared.get(helper_path))
    helper = import_retained('reviewed_final_fixture_job_owner', helper_path, helper_raw)
    guard_path = ROOT / protocol['guard_source']['path']
    guard_raw = regular(guard_path).read_bytes()
    require(hashlib.sha256(guard_raw).hexdigest() == declared[guard_path])
    guard = import_retained('reviewed_final_fixture_anchor_guard', guard_path, guard_raw)
    observer_path = ROOT / protocol['anchor_observer']['path']
    observer_raw = regular(observer_path).read_bytes()
    require(hashlib.sha256(observer_raw).hexdigest() == declared[observer_path])
    anchor = ROOT / protocol['fixture_anchor']
    require(Path(seal['output_anchor']) == anchor)
    observe = lambda path: guard.anchor_observation(path, observer_raw)
    proof, identity = guard.guard_output(anchor, anchor / 'materialized', [corpus_path, HERE], observe)
    argv = [seal['runtime_executable'], '-I', str(HERE / 'observer.py'), 'prepare',
            '--protocol', str(protocol_path), '--protocol-sha256', args.protocol_sha256,
            '--execute-reviewed-opaque-operation']
    result = helper.run_owned(argv, ROOT, log, 60)
    after = {}
    post_refused = False
    for path in before:
        try:
            after[path] = capture(path)
        except BaseException:
            post_refused = True
    anchor_unchanged = False
    try:
        anchor_unchanged = observe(anchor) == proof and (anchor.stat().st_dev, anchor.stat().st_ino) == identity
    except BaseException:
        pass
    counts_path = anchor / 'materialized/opaque-counts.json'
    counts = None
    output_hashes_match = False
    if counts_path.exists():
        require(regular(counts_path).stat().st_size <= 65536)
        counts = json.loads(counts_path.read_bytes())
        output_hashes_match = all(sha(regular(anchor / 'materialized' / name)) == counts[field]
                                 for name, field in [('manifest.private.json', 'manifest_sha256'),
                                                     ('rubric.private.json', 'rubric_sha256')])
    strict = strict_normal_close(helper, result)
    stable = not post_refused and before == after
    envelope_stable = (sha(protocol_path) == args.protocol_sha256 and sha(seal_path) == args.input_seal_sha256
                       and sha(helper_path) == args.job_helper_sha256)
    complete = bool(counts and counts.get('complete') is True and counts.get('tasks') == 205
                    and counts.get('hotpot') == 171 and counts.get('qasper') == 34
                    and counts.get('original_paragraphs') == 1685 and counts.get('goal_tasks') == 100)
    receipt = {'schema': 'airk.private-fresh-final-fixture-primary-execution/1',
               'protocol_sha256': args.protocol_sha256, 'input_seal_sha256': args.input_seal_sha256,
               'owned_result': result, 'strict_normal_close': strict, 'input_count': len(before),
               'inputs_unchanged': stable, 'source_runtime_protocol_helper_unchanged': envelope_stable,
               'anchor_acl_and_identity_unchanged': anchor_unchanged,
               'anchor_proof_sha256': hashlib.sha256(json.dumps(proof, sort_keys=True).encode()).hexdigest(),
               'emitted_manifest_rubric_stream_hashes_match': output_hashes_match, 'opaque': counts,
               'input_and_emitted_gold_content_parsed_by_observer': False,
               'sealed_input_and_emitted_metadata_bytes_stream_hashed_only': True,
               'native_or_NN_launched': False, 'quality_or_release_qualification': False,
               'original205_and_R1_R4_failed_unchanged': True}
    with args.execution_receipt.open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(receipt, stream, indent=2)
        stream.write('\n')
    passed = strict and stable and envelope_stable and anchor_unchanged and complete and output_hashes_match
    print(json.dumps({'complete': passed, 'strict_normal_close': strict, 'inputs_unchanged': stable,
                      'receipt_sha256': sha(args.execution_receipt),
                      'opaque_counts_sha256': sha(counts_path) if counts else None,
                      'tasks': counts.get('tasks') if counts else None,
                      'native_or_NN_launched': False}), flush=True)
    return 0 if passed else 1


if __name__ == '__main__':
    sys.exit(main())
