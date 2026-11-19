# SPDX-License-Identifier: Apache-2.0
"""Read opaque SHA-pinned native policy values; never model-policy authority."""
import hashlib
import json
from pathlib import Path
from admission import captured, parse

class NativePolicy:
    def __init__(self, pin):
        self.pin = dict(pin)
        self.path = Path(pin['path'])
        self.raw = captured(pin)
        native = parse(self.raw)
        self.value = native['policy']
        for field in ('deadline_ms', 'shared_input_tokens'):
            if type(self.value[field]) is not int or self.value[field] <= 0:
                raise ValueError('positive native policy declaration required: ' + field)
        self.deadline_ms = self.value['deadline_ms']
        self.deadline_seconds = self.deadline_ms / 1000
        self.shared_input_tokens = self.value['shared_input_tokens']

    def verify(self):
        if captured(self.pin) != self.raw:
            raise ValueError('native policy source changed')
