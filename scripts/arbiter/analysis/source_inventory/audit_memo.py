# SPDX-License-Identifier: Apache-2.0
"""Per-key memoization for pinned read-only audit inputs, never candidate outputs."""
from collections.abc import MutableMapping
from threading import RLock


class VerifiedMemo(MutableMapping):
    def __init__(self):
        self._values={};self._locks={};self._guard=RLock()
    def __getitem__(self,key):
        with self._guard:return self._values[key]
    def __setitem__(self,key,value):
        with self._guard:self._values[key]=value
    def __delitem__(self,key):
        with self._guard:del self._values[key]
    def __iter__(self):
        with self._guard:return iter(tuple(self._values))
    def __len__(self):
        with self._guard:return len(self._values)
    def snapshot(self):
        with self._guard:return dict(self._values)
    def get_or_compute(self,key,load):
        with self._guard:lock=self._locks.setdefault(key,RLock())
        with lock:
            with self._guard:
                if key in self._values:return self._values[key]
            value=load()
            with self._guard:self._values[key]=value
            return value


def cached(cache,key,load):
    if isinstance(cache,VerifiedMemo):return cache.get_or_compute(key,load)
    if key not in cache:cache[key]=load()
    return cache[key]
