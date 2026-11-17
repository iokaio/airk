# SPDX-License-Identifier: Apache-2.0
"""Root-released pure native vector plan; no model/session/NN execution."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import time
from prepare_fixture import ROOT, pin
from run_fixture import verify

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--fixture-packet', required=True)
    parser.add_argument('--variant', choices=('u8', 's8'), required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--receipt-directory', required=True)
    args = parser.parse_args()
    started = time.monotonic()
    packet = json.loads(Path(args.fixture_packet).read_text(encoding='utf-8'))
    verify(packet['image']); verify(packet['owner'])
    output = Path(args.output)
    if output.exists() or not output.is_absolute():
        raise ValueError('native draft output must be fresh and absolute')
    receipts = Path(args.receipt_directory)
    receipts.mkdir(exist_ok=False)
    model = ROOT / f'artifacts/advanced-split/bundled-model-v3/minilm-l6-v2-{args.variant}-v3'
    definition = pin(model / 'model.json')
    spec = importlib.util.spec_from_file_location('existing_owner', packet['owner']['path'])
    owner = importlib.util.module_from_spec(spec); spec.loader.exec_module(owner)
    environment = dict(os.environ)
    environment.update(AIRK_SELECTED_MINILM_DIRECTORY=str(model), AIRK_SELECTED_VECTOR_DRAFT_OUTPUT=str(output))
    argv = [packet['image']['path'], '--ignored', '--exact',
            'service_runtime::evaluation_fixture_tests::emit_private_selected_vector_configuration', '--nocapture']
    remaining = 60 - (time.monotonic() - started)
    if remaining <= 0:
        raise TimeoutError('pure native vector phase expired before launch')
    result = owner.run_owned(argv, ROOT, receipts / 'native.log', remaining, environment)
    receipt = {'schema': 'airk.private-selected-cal-vector-emitter/1', 'complete': False,
               'NN_invocations': 0, 'qualification_granted': False, 'image': packet['image'],
               'definition': definition, 'argv': argv, 'environment': {
                   key: environment[key] for key in ('AIRK_SELECTED_MINILM_DIRECTORY', 'AIRK_SELECTED_VECTOR_DRAFT_OUTPUT')},
               'owned': result}
    try:
        if not owner.successful(result):
            raise ValueError('native emitter failed/unsettled')
        log = (receipts / 'native.log').read_text(encoding='utf-8')
        if '1 passed;' not in log:
            raise ValueError('exact one native test pass not observed')
        verify(packet['image']); verify(packet['owner']); verify(definition)
        receipt['plan'] = pin(output / 'minilm-plan.json')
        if time.monotonic() - started >= 60:
            raise TimeoutError('native emitter original deadline expired during postchecks')
        receipt['complete'] = True
    finally:
        receipt['elapsed_seconds'] = time.monotonic() - started
        (receipts / 'execution.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')

if __name__ == '__main__':
    main()
