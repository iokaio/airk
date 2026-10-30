# SPDX-License-Identifier: Apache-2.0
"""Bounded retries for GitHub GET requests. Never replay publication mutations."""
from contextlib import nullcontext
import json
import re
import subprocess
import sys
import time


def get(path, *, paginate=False, missing=False, output=None):
    command = ["gh", "api", path, "--method", "GET"]
    if paginate:
        command += ["--paginate", "--slurp"]
    deadline = time.monotonic() + 90
    for attempt in range(4):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("GitHub read deadline expired")
        try:
            # Discard any partial download before retrying; authenticate bytes later.
            with output.open("wb") if output is not None else nullcontext(subprocess.PIPE) as destination:
                result = subprocess.run(command, stdout=destination, stderr=subprocess.PIPE,
                                        timeout=min(30, remaining), check=False)
            error = result.stderr.decode("utf-8", errors="replace") if isinstance(result.stderr, bytes) else result.stderr
            if result.returncode == 0:
                return None if output is not None else json.loads(result.stdout)
            if missing and re.search(r"HTTP 404\b", error):
                return None
            transient = bool(re.search(r"HTTP (408|429|500|502|503|504)\b", error)
                             or re.search(r"connection reset|TLS handshake timeout|i/o timeout|temporary failure in name resolution", error, re.I))
            failure = RuntimeError("GitHub GET failed: " + error.strip())
        except subprocess.TimeoutExpired as error:
            transient, failure = True, error
        delay = 2 ** attempt
        if not transient or attempt == 3 or time.monotonic() + delay >= deadline:
            raise failure
        print(f"Transient GitHub read failure; retry {attempt + 2}/4 in {delay}s", file=sys.stderr)
        time.sleep(delay)
