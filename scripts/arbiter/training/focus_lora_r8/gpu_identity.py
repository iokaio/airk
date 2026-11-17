# SPDX-License-Identifier: Apache-2.0
"""Pure strict physical GPU UUID and allocation metadata; no device API imports."""
import re

UUID=re.compile(r'[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\Z')


def canonical_uuid(value):
    if not isinstance(value,str):raise ValueError('GPU UUID must be text')
    raw=value.removeprefix('GPU-')
    if not UUID.fullmatch(raw):raise ValueError('physical GPU UUID format refused')
    return raw.lower()


def describe_gpu(name,raw_uuid,capability,free,total):
    try:canonical=canonical_uuid(raw_uuid)
    except ValueError:canonical=None
    return {'name':name,'uuid_raw':raw_uuid,'uuid_canonical':canonical,'compute_capability':capability,'free_bytes':free,'total_bytes':total}


def check_gpu_identity(expected,actual,minimum_free):
    if actual['uuid_canonical']is None or canonical_uuid(actual['uuid_raw'])!=actual['uuid_canonical']or actual['uuid_canonical']!=canonical_uuid(expected['uuid']):raise ValueError('actual physical GPU UUID differs')
    if actual['name']!=expected['name']:raise ValueError('actual GPU name differs')
    cap=actual['compute_capability']
    if not isinstance(cap,list)or len(cap)!=2 or any(type(x)is not int or x<0 for x in cap)or cap!=expected['compute_capability']:raise ValueError('actual GPU compute capability differs')
    if type(actual['free_bytes'])is not int or type(actual['total_bytes'])is not int or not 0<=actual['free_bytes']<=actual['total_bytes']or actual['free_bytes']<minimum_free:raise ValueError('actual GPU reserve differs')
    return True
