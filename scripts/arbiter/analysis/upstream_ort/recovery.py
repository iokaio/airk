# SPDX-License-Identifier: Apache-2.0
"""Compact exact-byte recovery records; no fuzzy patch application."""
import base64
import difflib
import hashlib
MAX_BYTES=16 << 20

def sha(raw):return hashlib.sha256(raw).hexdigest()

def recover(base,record):
    if set(record)!={'schema','base_sha256','base_bytes','target_sha256','target_bytes','edits'} or record['schema']!='airk.exact-byte-recovery/1':raise ValueError('wrong recovery schema')
    if len(base)>MAX_BYTES or len(base)!=record['base_bytes'] or sha(base)!=record['base_sha256']:raise ValueError('base identity differs')
    if type(record['target_bytes']) is not int or not 0<=record['target_bytes']<=MAX_BYTES:raise ValueError('target size exceeds bound')
    if not isinstance(record['edits'],list) or len(record['edits'])>100000:raise ValueError('edit roster exceeds bound')
    cursor=0;parts=[];size=0
    for edit in record['edits']:
        if set(edit)!={'offset','delete_bytes','deleted_sha256','insert_base64'}:raise ValueError('wrong edit schema')
        offset=edit['offset'];count=edit['delete_bytes']
        if type(offset) is not int or type(count) is not int or count<0 or offset<cursor or offset+count>len(base):raise ValueError('edit is overlapping or outside base')
        if sha(base[offset:offset+count])!=edit['deleted_sha256']:raise ValueError('deleted bytes differ')
        encoded=edit['insert_base64']
        if not isinstance(encoded,str) or len(encoded)>4*((MAX_BYTES+2)//3):raise ValueError('insert exceeds bound')
        inserted=base64.b64decode(encoded,validate=True)
        if base64.b64encode(inserted).decode()!=encoded:raise ValueError('noncanonical insert encoding')
        size+=offset-cursor+len(inserted)
        if size>MAX_BYTES:raise ValueError('recovered output exceeds bound')
        parts.extend((base[cursor:offset],inserted));cursor=offset+count
    parts.append(base[cursor:]);out=b''.join(parts)
    if len(out)!=record['target_bytes'] or sha(out)!=record['target_sha256']:raise ValueError('target identity differs')
    return out

def create(base,target):
    if max(len(base),len(target))>MAX_BYTES:raise ValueError('input exceeds bound')
    a=base.splitlines(keepends=True);b=target.splitlines(keepends=True);offsets=[0]
    for line in a:offsets.append(offsets[-1]+len(line))
    edits=[]
    for kind,i,j,k,l in difflib.SequenceMatcher(None,a,b,autojunk=False).get_opcodes():
        if kind=='equal':continue
        deleted=base[offsets[i]:offsets[j]]
        edits.append({'offset':offsets[i],'delete_bytes':len(deleted),'deleted_sha256':sha(deleted),'insert_base64':base64.b64encode(b''.join(b[k:l])).decode()})
    record={'schema':'airk.exact-byte-recovery/1','base_sha256':sha(base),'base_bytes':len(base),'target_sha256':sha(target),'target_bytes':len(target),'edits':edits}
    if recover(base,record)!=target:raise ValueError('generated recovery differs')
    return record
