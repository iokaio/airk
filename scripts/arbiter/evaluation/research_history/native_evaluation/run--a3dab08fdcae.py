# SPDX-License-Identifier: Apache-2.0
"""Bounded retained R10 native DEV capture; one family, no retry."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import time

ROOT = Path(__file__).resolve().parents[4]
def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--family', required=True)
    args = parser.parse_args()
    models = ROOT / 'artifacts/advanced-split/models'
    binary = ROOT / 'artifacts/advanced-split/native-harnesses/dev64-r10/airk-kernel.exe'
    descriptor = models / 'native-encoder-qualification-input-r1.json'
    corpus = models / 'a1-component-development-materialized-r1/corpus.private.json'
    mask = models / 'a1-component-dev-evaluation-protocol-r1/quality-mask.json'
    manifest = models / 'a1-component-dev-native-fixture-r1/manifest.private.json'
    owner = ROOT / 'tools/retrieval-train/training_execution.py'
    pins = {
        binary: '719bfd955566bacb856d7fd6d36668a6cf3074b62ac1c607fd1402c7ded7a26b',
        descriptor: '3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81',
        corpus: 'f6c6616c1de29948deee9f37c70ccaadbb7a5dc71d0cd02c91d207c9ccbac097',
        mask: '76842592dab174fd90bc3918542759adbf9cea615080334a308acafd6924727d',
        manifest: 'bf53b30e12a7aaa7b2bd4200ae892a8e2edefc2941dbb84c52483d62705dad2f',
        owner: 'd363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975',
    }
    for path, expected in pins.items():
        if sha(path) != expected:
            raise ValueError('reviewed input changed: ' + str(path))
    rows = json.loads(descriptor.read_bytes())
    row = next((row for row in rows if row['family'] == args.family), None)
    if row is None:
        raise ValueError('unknown closed baseline family')
    model_manifest = Path(row['directory']) / 'model.json'
    pins[model_manifest] = row['model_json_sha256']
    pins[Path(row['oracle'])] = row['oracle_sha256']
    assets = json.loads(model_manifest.read_bytes())
    for name, field in [('model.onnx', 'model_sha256'), ('tokenizer.json', 'tokenizer_sha256'), ('LICENSE', 'license_sha256')]:
        pins[model_manifest.parent / name] = assets[field]
    for path, expected in pins.items():
        if sha(path) != expected:
            raise ValueError('baseline asset drift: ' + str(path))
    work = models / 'native-dev64-baseline-r10-r1' / args.family
    work.mkdir(parents=True, exist_ok=False)
    (work / 'stdin.txt').write_bytes(b'')
    environment = dict(os.environ)
    environment.update({
        'AIRK_NATIVE_ENCODER_INPUT': str(descriptor), 'AIRK_NATIVE_ENCODER_FAMILY': args.family,
        'AIRK_NATIVE_DEV64_CORPUS': str(corpus), 'AIRK_NATIVE_DEV64_MASK': str(mask),
        'AIRK_NATIVE_DEV64_SOURCE_MANIFEST': str(manifest),
        'AIRK_NATIVE_DEV64_SOURCE_MANIFEST_SHA256': pins[manifest],
        'AIRK_NATIVE_DEV64_DATA': str(work / 'data'), 'AIRK_NATIVE_DEV64_OUTPUT': str(work / 'capture.json'),
        'AIRK_NATIVE_DEV64_STOP_FILE': str(work / 'STOP'),
        'OMP_NUM_THREADS': '2', 'MKL_NUM_THREADS': '2', 'OPENBLAS_NUM_THREADS': '2',
    })
    argv = [str(binary), '--exact', 'inference_owner::evaluation_tests::development::native_dev64_questions_use_current_kernel_with_fixed20_quality_mask', '--ignored', '--nocapture', '--test-threads=1']
    request = {'argv': argv, 'environment_overrides': {k: v for k, v in environment.items() if k.startswith('AIRK_NATIVE_') or k.endswith('NUM_THREADS')}, 'pins': {str(k): v for k, v in pins.items()}, 'limits': {'whole_seconds': 1800, 'host_bytes': 8 * 1024**3, 'job_processes': 2}, 'qualification': 'R10 retained identity; not current runtime; DEV only; original205 remains FAILED'}
    (work / 'request.json').write_text(json.dumps(request, indent=2) + '\n')
    spec = importlib.util.spec_from_file_location('owned', owner)
    owned = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owned)
    started = time.monotonic()
    with (work / 'stdout.log').open('wb') as log, (work / 'stdin.txt').open('rb') as stdin:
        result = owned.run_windows_owned_training(argv, ROOT, log, 1800, environment, stdin, 8 * 1024**3)
    drift = [str(path) for path, expected in pins.items() if sha(path) != expected]
    receipt = {'request': request, 'owned': result, 'elapsed_seconds': time.monotonic() - started, 'post_input_drift': drift}
    (work / 'execution.json').write_text(json.dumps(receipt, indent=2) + '\n')
    if result['exit_code'] or result['timed_out'] or result['owned_job_forced_termination'] or not result['owned_descendants_settled'] or drift:
        raise ValueError('native baseline failed; preserve partial result and stop')
    capture = json.loads((work / 'capture.json').read_bytes())
    if capture.get('complete') is not True or len(capture['records']) != 64:
        raise ValueError('native capture did not complete all64 requests')
    print(json.dumps({'family': args.family, 'records': 64, 'execution': str(work / 'execution.json'), 'capture_sha256': sha(work / 'capture.json'), 'elapsed_seconds': receipt['elapsed_seconds']}), flush=True)

if __name__ == '__main__':
    main()
