# SPDX-License-Identifier: Apache-2.0
"""Reuse original pre-Torch image/job/seal gate for separately released CPU quantization."""
import argparse
import json
from pathlib import Path
import sys

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execution-seal',type=Path,required=True);parser.add_argument('--execution-seal-sha256',required=True)
    parser.add_argument('--primary-release',type=Path,required=True);parser.add_argument('--primary-release-sha256',required=True)
    args=parser.parse_args();sys.path.insert(0,str(Path.cwd()/'tools/retrieval-train'))
    from training_execution import digest,verify_seal
    seal=verify_seal(args.execution_seal,args.execution_seal_sha256,wait_for_gate=True)
    if digest(args.primary_release)!=args.primary_release_sha256:raise ValueError('Root quantization release changed')
    release=json.loads(args.primary_release.read_bytes())
    if release['status']!='ROOT_RELEASED' or release['action']!='focus-static512-weight8' or release['limits']!=seal['limits']:raise ValueError('Distinct CPU quantization release missing')
    for name,expected in release['source_sha256'].items():
        if Path(name).name!=name or digest(Path(__file__).parent/name)!=expected:raise ValueError('Reviewed quantization source changed')
    from run_weight8 import run
    try:run(seal['focus_weight8_output'])
    finally:verify_seal(args.execution_seal,args.execution_seal_sha256)

if __name__=='__main__':main()
