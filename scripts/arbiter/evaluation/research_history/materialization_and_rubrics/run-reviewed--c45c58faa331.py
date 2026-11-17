# SPDX-License-Identifier: Apache-2.0
"""Primary-only reviewed bounded launch. Never reads emitted corpus or real gold."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path.cwd()
HERE = Path(__file__).resolve().parent


def sha(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def strict_normal_close(owner, result):
    return owner.successful(result) and result.get('owned_job_forced_termination') is False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input-seal', type=Path, required=True)
    parser.add_argument('--input-seal-sha256', required=True)
    parser.add_argument('--protocol-sha256', required=True)
    parser.add_argument('--job-helper-sha256', required=True)
    parser.add_argument('--execution-receipt', type=Path, required=True)
    parser.add_argument('--execute-reviewed-content-alignment', action='store_true', required=True)
    args = parser.parse_args()
    protocol = HERE / 'protocol.json'
    helper = ROOT / 'scripts/provision-static-onnx-runtime.py'
    if (args.execution_receipt.parent.resolve() != HERE or args.execution_receipt.exists() or sha(args.input_seal) != args.input_seal_sha256
            or sha(protocol) != args.protocol_sha256 or sha(helper) != args.job_helper_sha256):
        raise SystemExit('reviewed_launch_preflight_refused')
    seal = json.loads(args.input_seal.read_bytes())
    if Path(seal['output_anchor']) != ROOT / json.loads(protocol.read_bytes())['output_anchor']:
        raise SystemExit('foreign_output_anchor')
    before = {row['path']: sha(row['path']) for row in seal['files']}
    if any(before[row['path']] != row['sha256'] for row in seal['files']):
        raise SystemExit('reviewed_input_drift')
    # Run the exact helper bytes whose source SHA the primary/reviewer approved.
    raw = helper.read_bytes()
    if hashlib.sha256(raw).hexdigest() != args.job_helper_sha256:
        raise SystemExit('owned_helper_changed_before_import')
    import types
    module = types.ModuleType('reviewed_final_job_owner')
    module.__file__ = str(helper)
    exec(compile(raw, str(helper), 'exec'), module.__dict__)
    argv = [seal['runtime_executable'], '-I', str(HERE / 'adapter.py'),
        '--protocol', str(protocol), '--protocol-sha256', args.protocol_sha256,
        '--input-seal', str(args.input_seal), '--input-seal-sha256', args.input_seal_sha256,
        '--execute-reviewed-content-alignment']
    log = args.execution_receipt.with_suffix('.log')
    result = module.run_owned(argv, ROOT, log, 60)
    after = {path: sha(path) for path in before}
    settled = strict_normal_close(module, result)
    report_path = Path(seal['output_anchor']) / 'materialized/opaque-counts.json'
    opaque = json.loads(report_path.read_bytes()) if report_path.exists() else None
    # No corpus/gold file is opened by this observer, even to hash it again.
    receipt = {'schema': 'airk.private-fresh-final-primary-execution/1',
        'protocol_sha256': args.protocol_sha256, 'input_seal_sha256': args.input_seal_sha256,
        'owned_result': result, 'strict_normal_close': settled,
        'input_count': len(before), 'inputs_unchanged': before == after,
        'protocol_and_input_seal_unchanged': sha(protocol) == args.protocol_sha256 and sha(args.input_seal) == args.input_seal_sha256,
        'job_helper_unchanged': sha(helper) == args.job_helper_sha256,
        'emitted_final_gold_opened_by_observer': False, 'gold_content_parsed_by_observer': False,
        'sealed_input_bytes_stream_hashed_only': True, 'opaque': opaque,
        'original205_failed_unchanged': True, 'quality_qualification': False}
    with args.execution_receipt.open('x', encoding='utf-8') as output:
        json.dump(receipt, output, indent=2)
        output.write('\n')
    print(json.dumps({'strict_normal_close': settled, 'inputs_unchanged': before == after,
        'receipt_sha256': sha(args.execution_receipt),
        'opaque_counts_sha256': sha(report_path) if opaque else None,
        'combined_tasks': opaque.get('combined_tasks') if opaque else None,
        'gold_content_parsed_by_observer': False}))
    return 0 if settled and before == after and receipt['protocol_and_input_seal_unchanged'] and receipt['job_helper_unchanged'] and opaque and opaque.get('complete') and opaque.get('combined_tasks') == 205 else 1


if __name__ == '__main__':
    sys.exit(main())
