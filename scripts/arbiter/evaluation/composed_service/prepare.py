# SPDX-License-Identifier: Apache-2.0
"""Read-only maintained-source preflight; stdout is metadata, never a run permit."""
import argparse
import hashlib
import json
from pathlib import Path
from admission import captured, parse, validate_plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', required=True, type=Path)
    parser.add_argument('--plan-sha256', required=True)
    args = parser.parse_args()
    raw = captured({'path': str(args.plan), 'sha256': args.plan_sha256})
    plan = parse(raw)
    validate_plan(plan)
    # Reopen after all consumed inputs are checked; never relabel a changed plan.
    if captured({'path': str(args.plan), 'sha256': args.plan_sha256}) != raw:
        raise ValueError('original orchestration plan changed')
    print(json.dumps({'schema': 'airk.composed-preflight/1',
                      'plan': {'path': str(args.plan), 'sha256': hashlib.sha256(raw).hexdigest(), 'bytes': len(raw)},
                      'source_seals': plan['source_seals'],
                      'status': 'source_inputs_verified_execution_owner_required',
                      'execution_authorized': False, 'qualification_granted': False}, sort_keys=True))


if __name__ == '__main__':
    main()
