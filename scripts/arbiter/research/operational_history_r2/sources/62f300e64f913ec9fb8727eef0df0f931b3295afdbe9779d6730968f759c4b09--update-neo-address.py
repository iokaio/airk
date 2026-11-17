# SPDX-License-Identifier: Apache-2.0
"""Update a user-confirmed address, preserving the existing trusted Mac key."""
import datetime
import json
from pathlib import Path
import subprocess

record = Path(__file__).resolve().parent / 'neo-connection.local.json'
connection = json.loads(record.read_text(encoding='utf-8-sig'))
old, new = connection['host'], '192.168.68.61'
assert old == '192.168.68.50'
common = ['ssh', '-i', connection['identity_path'], '-p', str(connection['port']),
          *connection['ssh_options']]
subprocess.run([*common, '-o', 'HostKeyAlias=' + old,
                connection['user'] + '@' + new, '/usr/bin/uname -sm'], check=True)
known = Path(connection['identity_path']).parent / 'known_hosts'

def keys(host):
    found = subprocess.run(['ssh-keygen', '-F', host, '-f', str(known)],
                           capture_output=True, text=True)
    assert found.returncode in (0, 1), found.stderr
    result = set()
    for line in found.stdout.splitlines():
        fields = line.split()
        if not fields or fields[0].startswith('#'):
            continue
        assert not fields[0].startswith('@'), 'Marker requires separate host-key review'
        assert len(fields) >= 3
        result.add((fields[1], fields[2]))
    return result

trusted = keys(old)
assert trusted, 'Previously trusted Mac key unavailable'
missing = trusted - keys(new)
if missing:
    # Append only the already trusted Mac public keys for its confirmed new IP.
    # Preserve all other entries; never read or copy the private identity key.
    with known.open('ab') as output:
        for algorithm, key in sorted(missing):
            output.write(('\n' + new + ' ' + algorithm + ' ' + key + '\n').encode('ascii'))
subprocess.run([*common, connection['user'] + '@' + new, '/usr/bin/uname -sm'], check=True)
connection['host'] = new
connection['known_command'] = connection['known_command'].replace(old, new)
connection['address_update'] = {
    'previous': old, 'current': new,
    'verified_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'basis': 'User confirmed router reassignment; existing trusted Mac key verified before update',
}
record.write_text(json.dumps(connection, indent=2) + '\n', encoding='utf-8')
print('Saved Mac Neo address updated to ' + new + '; existing trusted host key preserved.')
