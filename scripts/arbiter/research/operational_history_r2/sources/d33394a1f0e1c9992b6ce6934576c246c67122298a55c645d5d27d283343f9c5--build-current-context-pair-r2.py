# SPDX-License-Identifier: Apache-2.0
"""Prepared root-only matched debug pair using authentic owned ORT R5 inputs."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path.cwd()
PRODUCER_SHA256 = 'a1772271cfc3e585de903a7e80021b54f97da823a485e48621a60dcee564c044'
ENV_SHA256 = '0edd26b2f85e2ca84b24f2bfd83855ffebb593697e247532684c10e749aef870'
ADMIN_INVENTORY_SHA256 = '9606e4c05c6a90d87d9941a1c299bcf0ca465ed961cdb66c19ffb78120542388'
DESTINATION = ROOT / 'artifacts/advanced-split/runtime-current-context-r2'
RECEIPTS = ROOT / 'artifacts/advanced-split/receipts/current-context-pair-build-r2'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def main():
    assert not DESTINATION.exists() and not RECEIPTS.exists()
    assert sha((ROOT / 'scripts/provision-static-onnx-runtime.py').read_bytes()) == PRODUCER_SHA256
    env_path = ROOT / 'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json'
    env_raw = env_path.read_bytes()
    assert sha(env_raw) == ENV_SHA256
    handoff = json.loads(env_raw)
    selected = handoff['environment']
    assert set(selected) == {'LIBONNXRUNTIME_NO_PKG_CONFIG', 'ORT_LIB_LOCATION', 'ORT_LIB_PROFILE',
                             'ORT_PREFER_DYNAMIC_LINK', 'ORT_SKIP_DOWNLOAD'}
    assert selected['ORT_LIB_LOCATION'] == str(ROOT / 'artifacts/advanced-split/ort130-x64-owned-r5/aggregate')
    assert selected['ORT_LIB_PROFILE'] == 'Release'
    assert selected['LIBONNXRUNTIME_NO_PKG_CONFIG'] == selected['ORT_SKIP_DOWNLOAD'] == '1'
    assert selected['ORT_PREFER_DYNAMIC_LINK'] == '0'
    assert sha((ROOT / 'artifacts/admin/admin-assets.json').read_bytes()) == ADMIN_INVENTORY_SHA256
    env = {key: value for key, value in os.environ.items() if not key.upper().startswith('GIT_')}
    env.update(selected)
    env.update(CARGO_BUILD_JOBS='8', CARGO_INCREMENTAL='1', CARGO_NET_OFFLINE='true',
               AIRK_BUNDLED_MODEL_DIR=str(ROOT / 'artifacts/advanced-split/bundled-model-v2'))
    command = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures,frontend',
               '--tier', 'T2', '--out', str(RECEIPTS), '--target-dir', 'artifacts/v190/p10-target',
               '--node', str(ROOT / 'artifacts/tools/node-v24.15.0-win-x64/node.exe'), '--allow-no-tests',
               'matched-pair-build', sys.executable, 'scripts/build-service-runtime.py',
               '--destination', str(DESTINATION), '--profile', 'debug',
               '--admin-inventory', 'artifacts/admin/admin-assets.json']
    return subprocess.run(command, env=env).returncode


if __name__ == '__main__':
    sys.exit(main())
