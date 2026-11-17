# SPDX-License-Identifier: Apache-2.0
"""Use the canonical unchanged 84-gate evaluator against P8 and frozen controls."""
import argparse
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'artifacts/v190'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate-off', type=Path, default=BASE / 'p9-off-mcp/replay/report.json')
    parser.add_argument('--candidate-report', type=Path, default=BASE / 'p9-report-mcp/replay/report.json')
    parser.add_argument('--output', type=Path, default=BASE / 'p9-gates.json')
    args = parser.parse_args()
    command = [sys.executable, str(ROOT / 'scripts/benchmark-mcp-investigation.py'), 'phase-gates',
               '--baseline', str(ROOT / 'docs/research/baseline-v190.json'),
               '--candidate-off', str(args.candidate_off), '--candidate-report', str(args.candidate_report)]
    for label, prefix in [('archive', 'p5-188'), ('control', 'p6f-final-control'), ('preceding', 'p8')]:
        for mode in ('off', 'report'):
            command.extend([f'--{label}-{mode}', str(BASE / f'{prefix}-{mode}-mcp/replay/report.json')])
    command.extend(['--output', str(args.output)])
    raise SystemExit(subprocess.run(command, cwd=ROOT).returncode)


if __name__ == '__main__':
    main()
