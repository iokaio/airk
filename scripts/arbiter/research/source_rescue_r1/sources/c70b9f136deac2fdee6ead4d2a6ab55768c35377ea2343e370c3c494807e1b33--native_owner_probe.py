# SPDX-License-Identifier: Apache-2.0
"""PRIVATE single-child caller guard probe, no AIRK or numerical libraries."""
import json
from pathlib import Path
import sys
import custody
worker=custody.load_worker('9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7')
Path(sys.argv[1]).write_text('actual suspended-authenticated child resumed',encoding='utf-8')
print(json.dumps({'actual_sid':worker.windows_sid(),'actual_job':custody.current_job()}),flush=True)
