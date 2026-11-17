# SPDX-License-Identifier: Apache-2.0
"""Consume the SHA-pinned Rust-emitted budget fixture; no native contract defaults."""
import hashlib,json
from pathlib import Path
class NativeDelivery:
    def __init__(self,pin,policy):
        self.pin=dict(pin);self.path=Path(pin['path'])
        if not self.path.is_absolute() or self.path.is_symlink() or not self.path.is_file():raise ValueError('original native delivery fixture required')
        self.raw=self.path.read_bytes()
        if len(self.raw)>16777216 or hashlib.sha256(self.raw).hexdigest()!=pin['sha256']:raise ValueError('native delivery fixture bytes differ')
        value=json.loads(self.raw)
        if (value.get('schema')!='airk.arbiter.budget-cases/1'
            or value['operation_deadline_ms']!=policy.deadline_ms
            or value['default_budget']['input_tokens']!=policy.shared_input_tokens
            or type(value['transport_deadline_ms']) is not int
            or value['transport_deadline_ms']<policy.deadline_ms):raise ValueError('native delivery/policy sources differ')
        self.deadline_ms=value['transport_deadline_ms'];self.deadline_seconds=self.deadline_ms/1000
    def verify(self):
        if self.path.read_bytes()!=self.raw:raise ValueError('native delivery fixture changed')
