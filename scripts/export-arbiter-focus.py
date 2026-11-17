# SPDX-License-Identifier: Apache-2.0
"""Mechanical FP32 adapted Focus export only. Caller owns identity/process/output custody.
No training, reference generation, Torch/ORT import, model call or qualification.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import time


def file_hash(path, deadline):
    h = hashlib.sha256()
    with Path(path).open('rb') as source:
        while raw := source.read(4 << 20):
            if time.monotonic() >= deadline:
                raise ValueError('original export deadline expired')
            h.update(raw)
    return h.hexdigest()


def unique(pairs):
    out = {}
    for key, value in pairs:
        if key in out:
            raise ValueError('duplicate source-pin key')
        out[key] = value
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('source', 'publisher', 'publisher-config', 'merged', 'checkpoint-custody', 'source-pins', 'python-site', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--source-pins-sha256', required=True)
    parser.add_argument('--deadline-monotonic-ns', type=int, required=True)
    args = parser.parse_args()
    deadline = args.deadline_monotonic_ns / 1_000_000_000
    if not 0 < deadline - time.monotonic():
        raise ValueError('original export deadline expired')
    if not args.source_pins.is_file() or args.source_pins.stat().st_size > 4 << 20:
        raise ValueError('bounded explicit source pins required')
    raw = args.source_pins.read_bytes()
    if hashlib.sha256(raw).hexdigest() != args.source_pins_sha256:
        raise ValueError('source pin file differs')
    pins = json.loads(raw, object_pairs_hook=unique)
    module_path = Path(__file__).resolve().with_name('arbiter-focus-export.py')
    code = [Path(__file__).resolve(), module_path]
    for path in code:
        if file_hash(path, deadline) != pins.get(str(path)):
            raise ValueError('caller must seal exact ordinary exporter code')
    spec = importlib.util.spec_from_file_location('airk_focus_fp32_export', module_path)
    exporter = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(exporter)
    exporter.ordinary(args.source_pins)
    site = args.python_site
    if not site.is_absolute() or site.resolve(strict=True) != site:
        raise ValueError('explicit existing caller-sealed Python site required')
    for path in (site, *site.parents):
        if path.is_symlink() or getattr(path.lstat(), 'st_file_attributes', 0) & 0x400:
            raise ValueError('Python site alias refused')
    sys.path.insert(0, str(site))
    import onnx
    import numpy
    result = exporter.export_after_owner_admission(args.source, args.publisher, args.publisher_config,
                    args.merged, args.checkpoint_custody, pins, args.output, onnx, numpy, deadline)
    for path in code:
        if file_hash(path, deadline) != pins[str(path)]:
            raise ValueError('exporter code changed during conversion')
    if file_hash(args.source_pins, deadline) != args.source_pins_sha256 or time.monotonic() >= deadline:
        raise ValueError('original input/deadline changed during final retention')
    print(json.dumps({'output': str(args.output), 'files': result['files'],
                      'qualification': False, 'model_inference': False}, indent=2))


if __name__ == '__main__':
    main()
