# SPDX-License-Identifier: Apache-2.0
"""Sealed pre-Torch gate for the root's one actual publisher-reference trial."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execution-seal',type=Path,required=True)
    parser.add_argument('--execution-seal-sha256',required=True)
    parser.add_argument('--primary-release',type=Path,required=True)
    parser.add_argument('--primary-release-sha256',required=True)
    args=parser.parse_args()
    root=Path.cwd()
    sys.path.insert(0,str(root/'tools/retrieval-train'))
    from training_execution import digest,verify_seal,verify_torch_runtime
    seal=verify_seal(args.execution_seal,args.execution_seal_sha256,wait_for_gate=True)
    if digest(args.primary_release)!=args.primary_release_sha256:raise ValueError('Root release changed')
    release=json.loads(args.primary_release.read_bytes())
    if release['status']!='ROOT_RELEASED' or release['action']!='focus-publisher-reference':raise ValueError('Root reference release absent')
    for name in ['run_focus_export.py','focus_reference.py','owned_reference.py']:
        if digest(Path(__file__).parent/name)!=release['source_sha256'][name]:raise ValueError('Released Focus source changed')
    if release['limits']!=seal['limits']:raise ValueError('Released resource limits differ')
    if any(name in sys.modules for name in ['torch','transformers']):raise ValueError('Model runtime imported before sealed gate')
    import torch
    verify_torch_runtime(torch,args.execution_seal)
    import run_focus_export
    sys.argv=[str(Path(run_focus_export.__file__).resolve()),'--phase','reference','--tokens','128','--threads','1','--device','cuda','--output',seal['focus_output']]
    try:
        run_focus_export.main()
    finally:
        verify_seal(args.execution_seal,args.execution_seal_sha256)
    receipt=json.loads((Path(seal['focus_output'])/'receipt.json').read_bytes())
    if receipt.get('complete') is not True or len(receipt['rows'])!=6:raise ValueError('Full reference did not complete all controls')

if __name__=='__main__':main()
