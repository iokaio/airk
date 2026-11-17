# SPDX-License-Identifier: Apache-2.0
"""Blocked-base gate for Root-owned zero-forward copy/storage normalization."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys

def sha(path):
    with Path(path).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execution-seal',type=Path,required=True);parser.add_argument('--execution-seal-sha256',required=True)
    args=parser.parse_args()
    if sha(args.execution_seal)!=args.execution_seal_sha256:raise ValueError('Reviewed no-NN seal differs')
    seal=json.loads(args.execution_seal.read_bytes())
    if seal['status']!='ROOT_RELEASED_ZERO_FORWARD_NORMALIZATION' or os.environ.get('AIRK_TRAIN_START_GATE')!='1':raise ValueError('Root release/owned gate missing')
    print('AIRK_TRAIN_GATE_READY:'+str(os.getpid()),flush=True)
    if sys.stdin.readline().strip()!='GO '+args.execution_seal_sha256:raise ValueError('Actual owned base GO missing')
    def verify():
        for name,expected in seal['environment'].items():
            if os.environ.get(name)!=expected:raise ValueError('No-NN environment differs: '+name)
        if any(Path(seal['environment']['PYTHONPYCACHEPREFIX']).glob('**/*')):raise ValueError('Unexpected executable bytecode cache')
        for row in seal['files']:
            if sha(row['path'])!=row['sha256']:raise ValueError('Normalization source/runtime/artifact drift: '+row['path'])
    verify()
    if any(name in sys.modules for name in ['torch','onnxruntime']):raise ValueError('Inference runtime imported into no-NN normalization')
    import normalize_focus_artifact
    sys.argv=[str(Path(normalize_focus_artifact.__file__).resolve()),'--output',seal['normalization_output'],'--source',seal['normalization_source']]
    try:normalize_focus_artifact.main()
    finally:verify()
    if any(name in sys.modules for name in ['torch','onnxruntime']):raise ValueError('Inference runtime imported during normalization')
if __name__=='__main__':main()
