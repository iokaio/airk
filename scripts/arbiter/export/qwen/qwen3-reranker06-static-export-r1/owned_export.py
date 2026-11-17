# SPDX-License-Identifier: Apache-2.0
"""Original sealed pre-Torch process gate, with an independently released CPU export."""
import argparse
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
    sys.path.insert(0,str(Path.cwd()/'tools/retrieval-train'))
    from training_execution import digest,verify_seal
    seal=verify_seal(args.execution_seal,args.execution_seal_sha256,wait_for_gate=True)
    if digest(args.primary_release)!=args.primary_release_sha256:raise ValueError('Root export release changed')
    release=json.loads(args.primary_release.read_bytes())
    if release['status']!='ROOT_RELEASED' or release['action']!='focus-static-fp32-export' or release['limits']!=seal['limits']:
        raise ValueError('Independent Root CPU export release absent')
    for name,expected in release['source_sha256'].items():
        if Path(name).name!=name or digest(Path(__file__).parent/name)!=expected:raise ValueError('Released export source changed')
    if any(name in sys.modules for name in ['torch','transformers']):raise ValueError('Model runtime imported before sealed gate')
    from run_static_export import run
    try:run(seal['focus_export_output'])
    finally:verify_seal(args.execution_seal,args.execution_seal_sha256)

if __name__=='__main__':main()
