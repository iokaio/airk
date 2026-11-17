# SPDX-License-Identifier: Apache-2.0
"""Preserve a Python probe encoded in a raw UTF-8 traceback; never execute it."""
import ast
import base64
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
BASE=Path(__file__).resolve().parent
ROOT=BASE.parents[3]
SOURCE='artifacts/local/one-airk-1814/neo-final-preparation-1794097748/stderr.txt'
METHOD='single-python-b64decode-literal/1'

def sha(raw):return hashlib.sha256(raw).hexdigest()
def decode(raw):
    field=raw.decode('utf-8',errors='strict')
    matches=re.findall(r"b64decode\('([A-Za-z0-9+/=]+)'\)",field)
    if len(matches)!=1 or field.count('b64decode(')!=1:raise ValueError('Expected exactly one base64 literal')
    body=base64.b64decode(matches[0],validate=True);body.decode('utf-8',errors='strict');ast.parse(body)
    return body

def plan():
    raw=(ROOT/SOURCE).read_bytes();body=decode(raw);digest=sha(body)
    if digest!='c51f8acec624fab63804a5655a1c910df2d6c1a4128e0d71665ecde7e9ae1a68':raise ValueError('Reviewed probe body drift')
    prefix='# SPDX-License-Identifier: Apache-2.0\n';target=(BASE/('neo-final-preparation--'+digest[:12]+'.py.in')).relative_to(ROOT).as_posix();payload=prefix.encode()+body
    blob=hashlib.sha1(b'blob '+str(len(body)).encode()+b'\0'+body).hexdigest();reachable={x.split(' ',1)[0] for x in subprocess.run(['git','rev-list','--objects','--all'],cwd=ROOT,capture_output=True,text=True,check=True).stdout.splitlines()}
    row=dict(container=SOURCE,container_format='utf8-text',container_sha256=sha(raw),container_bytes=len(raw),json_pointer='',encoded_field_sha256=sha(raw),encoded_field_bytes=len(raw),decode_method=METHOD,decoded_sha256=digest,decoded_bytes=len(body),destination=target,prefix=prefix,maintained_sha256=sha(payload),maintained_bytes=len(payload),language='python',disposition='extracted-source',exact_git_blob=blob,git_blob_reachable_at_plan=blob in reachable)
    return {'schema':'airk.encoded-source-migration/1','scope':'One reviewed raw-text container; empty pointer selects its complete strict UTF-8 field. Traceback/result bytes are not copied.','files':[row]}

def apply(doc):
    if doc.get('schema')!='airk.encoded-source-migration/1' or len(doc['files'])!=1 or (BASE/'encoded-source-provenance.json').exists():raise ValueError('Unexpected schema/roster or existing provenance')
    row=doc['files'][0]
    if row['container']!=SOURCE or row['container_format']!='utf8-text' or row['json_pointer']!='' or row['decode_method']!=METHOD or row['disposition']!='extracted-source':raise ValueError('Unsupported decode contract')
    raw=(ROOT/SOURCE).read_bytes()
    if sha(raw)!=row['container_sha256'] or len(raw)!=row['container_bytes'] or sha(raw)!=row['encoded_field_sha256'] or len(raw)!=row['encoded_field_bytes']:raise ValueError('Container/field drift')
    body=decode(raw);payload=row['prefix'].encode()+body;target=(ROOT/row['destination']).resolve()
    if sha(body)!=row['decoded_sha256'] or len(body)!=row['decoded_bytes'] or sha(payload)!=row['maintained_sha256'] or len(payload)!=row['maintained_bytes']:raise ValueError('Source drift')
    if target.parent!=BASE or target.exists():raise ValueError('Unsafe output')
    # Caller preflights both output parents and audits created outputs.
    target.write_bytes(payload);(BASE/'encoded-source-provenance.json').write_text(json.dumps(doc,indent=2)+'\n',encoding='utf-8')
if __name__=='__main__':
    if sys.argv[1:]==['apply']:apply(json.load(sys.stdin))
    elif not sys.argv[1:]:print(json.dumps(plan()))
    else:raise SystemExit('usage: decode_probe.py [apply]')
