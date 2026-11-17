# SPDX-License-Identifier: Apache-2.0
"""Read opaque SHA-pinned native policy values; never model-policy authority."""
import hashlib
import json
from pathlib import Path

class NativePolicy:
    def __init__(self, pin):
        self.pin = dict(pin)
        self.path = Path(pin['path'])
        if not self.path.is_absolute() or self.path.is_symlink() or not self.path.is_file():
            raise ValueError('original ordinary native policy input required')
        if self.path.stat().st_size > 16 * 1024 * 1024:
            raise ValueError('native policy input exceeds bounded metadata size')
        self.raw = self.path.read_bytes()
        if hashlib.sha256(self.raw).hexdigest() != pin['sha256']:
            raise ValueError('native policy input digest differs')
        native = json.loads(self.raw)
        self.value = native['policy']
        for field in ('deadline_ms', 'shared_input_tokens'):
            if type(self.value[field]) is not int or self.value[field] <= 0:
                raise ValueError('positive native policy declaration required: ' + field)
        self.deadline_ms = self.value['deadline_ms']
        self.deadline_seconds = self.deadline_ms / 1000
        self.shared_input_tokens = self.value['shared_input_tokens']

    def verify(self):
        if self.path.read_bytes() != self.raw:
            raise ValueError('native policy source changed')
