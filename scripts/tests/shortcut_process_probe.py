# SPDX-License-Identifier: Apache-2.0
"""Child CLI stand-in for launcher forwarding tests; never runs AIRK."""
import json
import sys
print(json.dumps({"arguments": sys.argv[1:], "stdin": sys.stdin.read()}, ensure_ascii=False))
print("shortcut stderr", file=sys.stderr)
raise SystemExit(37)
