# SPDX-License-Identifier: Apache-2.0
import json, pathlib, subprocess, sys
root = pathlib.Path(__file__).resolve().parents[2]
note = json.loads((root / 'artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'))
assert sys.argv[1:] in [['inventory'], ['clean'], ['profile_probe'], ['verified_clean']]
remote = '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python - ' + sys.argv[1]
command = ['ssh', '-i', note['identity_path'], '-p', str(note['port']), *note['ssh_options'], note['user'] + '@' + note['host'], remote]
payload = (root / 'artifacts/v190/p9-neo-cargo-clean-remote.py').read_bytes()
raise SystemExit(subprocess.run(command, input=payload).returncode)
