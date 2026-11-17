# SPDX-License-Identifier: Apache-2.0
"""Unchanged gate evaluator with explicit actual preceding-phase observations."""
import argparse, subprocess, sys
from pathlib import Path
root = Path(__file__).resolve().parents[2]
base = root/'artifacts/v190'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--candidate-off',type=Path,required=True)
parser.add_argument('--candidate-report',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--preceding-off',type=Path,default=base/'p10-final-off-mcp-r6/replay/report.json')
parser.add_argument('--preceding-report',type=Path,default=base/'p10-final-report-mcp-r6/replay/report.json')
args = parser.parse_args()
cmd = [sys.executable,str(root/'scripts/benchmark-mcp-investigation.py'),'phase-gates',
    '--baseline',str(root/'docs/research/baseline-v190.json'),
    '--candidate-off',str(args.candidate_off),'--candidate-report',str(args.candidate_report)]
for label, paths in [('archive',['p5-188-off-mcp','p5-188-report-mcp']),
    ('control',['p6f-final-control-off-mcp','p6f-final-control-report-mcp'])]:
    for mode,path in zip(('off','report'),paths):
        cmd.extend([f'--{label}-{mode}',str(base/path/'replay/report.json')])
cmd.extend(['--preceding-off',str(args.preceding_off),'--preceding-report',str(args.preceding_report)])
cmd.extend(['--output',str(args.output)])
sys.exit(subprocess.run(cmd,cwd=root).returncode)
