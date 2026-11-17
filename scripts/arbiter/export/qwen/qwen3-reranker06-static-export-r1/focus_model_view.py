# SPDX-License-Identifier: Apache-2.0
"""Approved model-view quoting only; original source bytes remain authoritative."""
def model_view(text):
    return ''.join({'\\':'\\\\','<':r'\u003c','>':r'\u003e'}.get(char,char)for char in text)
