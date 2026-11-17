# SPDX-License-Identifier: Apache-2.0
"""Child of existing owned runner; SSH inherits that process ownership."""
import hashlib,json,subprocess,sys
from pathlib import Path
request=json.loads(Path(sys.argv[1]).read_text());root=Path(request['root']);path=Path(request['script']);raw=path.read_bytes();assert hashlib.sha256(raw).hexdigest()==request['sha256']
c=json.loads((root/'artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host'],'sh','-s']
r=subprocess.run(ssh,input=raw,stdout=sys.stdout.buffer,stderr=sys.stderr.buffer);raise SystemExit(r.returncode)
