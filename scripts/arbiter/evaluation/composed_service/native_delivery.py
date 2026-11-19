# SPDX-License-Identifier: Apache-2.0
"""Consume the SHA-pinned Rust-emitted budget fixture; no native contract defaults."""
import hashlib,json
from pathlib import Path
from admission import captured, parse
class NativeDelivery:
    def __init__(self,pin,policy):
        self.pin=dict(pin);self.path=Path(pin['path'])
        self.raw=captured(pin)
        value=parse(self.raw)
        if (value.get('schema')!='airk.arbiter.budget-cases/1'
            or value['operation_deadline_ms']!=policy.deadline_ms
            or value['default_budget']['input_tokens']!=policy.shared_input_tokens
            or type(value['transport_deadline_ms']) is not int
            or value['transport_deadline_ms']<policy.deadline_ms):raise ValueError('native delivery/policy sources differ')
        self.deadline_ms=value['transport_deadline_ms'];self.deadline_seconds=self.deadline_ms/1000
    def verify(self):
        if captured(self.pin)!=self.raw:raise ValueError('native delivery fixture changed')
