# SPDX-License-Identifier: Apache-2.0
from dataclasses import dataclass


@dataclass
class Token:
    value: str


def authenticate(token: Token) -> bool:
    return token.value == "fictional-example"
